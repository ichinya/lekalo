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
    Freshness, NativeBinding, Projection, QueryRequest, SearchInputs, SearchStore,
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
    let cap = lekalo_core::search::MAX_POSTINGS;
    let index = match lekalo_core::search::SearchIndex::from_projection(
        &service.projection,
        &service.pins,
        &service.policy_digest,
        service.project_id.as_deref(),
        cap,
    ) {
        Ok(index) => index,
        Err(set) => return DomainResult::invalid(set),
    };
    let document = snapshot_document(&service);
    let index_digest = format!("sha256:{}", lekalo_core::search::digest_of(&document));
    // Coverage honesty: a needed domain that is not complete is warned
    // about once per domain so absence is never read as proof.
    let mut diagnostics: Vec<lekalo_core::diagnostics::Diagnostic> = Vec::new();
    for row in &service.coverage {
        if row.state != "complete" {
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
        service.coverage,
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
        lekalo_core::search::MAX_POSTINGS,
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
fn assemble(page: &PageArgs, no_cache: bool) -> Result<Service, DomainResult> {
    let selection = lekalo_core::loader::LoadSelection {
        project: page
            .project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let (_model, compilation) = lekalo_core::cache::load_compiled(&selection, no_cache)?;
    let graph = lekalo_core::effects::build(&compilation.project).map_err(DomainResult::invalid)?;
    let cap = lekalo_core::search::MAX_POSTINGS;
    let mut projection = project_ir(&compilation.project, cap);
    attach_effects(&mut projection, &graph, cap);
    let mut coverage: Vec<lekalo_core::search::CoverageRow> = vec![
        coverage_row("ir", "complete"),
        coverage_row("effects", "complete"),
    ];
    let root = lekalo_core::doctor::project_root(&selection)
        .map_err(|result: DomainResult| result)
        .ok();
    let mut pins: Vec<lekalo_core::search::InputPin> = Vec::new();
    let mut dropped_rows: u64 = 0;
    match &page.inputs {
        Some(path) => {
            let bytes = read_admitted(path).map_err(|detail| {
                DomainResult::invalid(lekalo_core::search::diagnostic_query_invalid(
                    "inputs", detail,
                ))
            })?;
            let manifest = SearchInputs::parse(&bytes).map_err(DomainResult::invalid)?;
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
            // The consumed input pins ride the snapshot and the policy
            // digest so a generation can prove what it was built from;
            // each pin records the exact live bytes digest it consumed.
            pins = lekalo_core::search::pins_of(&manifest)
                .into_iter()
                .map(|mut pin| {
                    if let Some(root) = &root {
                        if let Ok(produced) = std::fs::read(root.join(&pin.path)) {
                            pin.digest = Some(format!(
                                "sha256:{}",
                                lekalo_core::search::sha256_of(&produced)
                            ));
                        }
                    }
                    pin
                })
                .collect();
            dropped_rows = admit_producers(
                &selection,
                project_id.as_deref(),
                &manifest,
                &mut projection,
                &mut coverage,
            )?;
        }
        None => {
            // Without an explicit manifest the optional producers are
            // reported honestly instead of silently complete.
            for domain in [
                "trace",
                "requirements",
                "bindings",
                "diagnostics",
                "waivers",
            ] {
                coverage.push(coverage_row(domain, "unknown"));
            }
        }
    }
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
        dropped_rows,
    };
    Ok(Service {
        policy_digest: service.compute_policy_digest(),
        ..service
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

/// The custody envelope the effects producer document must carry: the
/// exact published contract identity plus its declared project.
struct EffectsCustody {
    project: String,
}

/// Run the effects producer document through its published typed
/// decoder (`lekalo/effects/v0.2.16`): contract identity, closed row
/// grammar, and envelope cross-checks (metadata counts, project) are
/// enforced before any row is projected.
fn decode_effects_envelope(
    _bytes: &[u8],
    value: &serde_json::Value,
) -> Result<EffectsCustody, DomainResult> {
    const EFFECTS_IDENTITY: &str = lekalo_core::effects::version::IDENTITY;
    const EFFECTS_SCHEMA: &str = lekalo_core::effects::version::SCHEMA_VERSION;
    let identity = value
        .get("identity")
        .and_then(|value| value.as_str())
        .ok_or_else(|| producer_invalid("producer-shape", "effects"))?;
    let schema_version = value
        .get("schemaVersion")
        .and_then(|value| value.as_str())
        .ok_or_else(|| producer_invalid("producer-shape", "effects"))?;
    if identity != EFFECTS_IDENTITY || schema_version != EFFECTS_SCHEMA {
        return Err(DomainResult::invalid(
            lekalo_core::search::diagnostic_input_invalid("identity-mismatch", Some("effects")),
        ));
    }
    // The published typed row decoder refuses any malformed row (closed
    // grammar over operation/subject/kind/provenance), so no hand-picked
    // row can silently enter the projection.
    let detected_rows = decode_effects_rows(value)?;
    // The declared rows are rebuilt from the canonical IR at admission,
    // so only the producer's own detected rows are custody-checked here;
    // a snapshot that also carries declared rows is a foreign shape.
    let declared = value
        .get("effects")
        .and_then(|effects| effects.as_array())
        .map(|rows| {
            rows.iter()
                .filter(|row| {
                    row.get("provenance")
                        .and_then(|provenance| provenance.get("type"))
                        .and_then(|value| value.as_str())
                        == Some("canonical-ir")
                })
                .count()
        })
        .unwrap_or(0);
    if declared > 0 {
        return Err(producer_invalid("producer-shape", "effects"));
    }
    let detected = value
        .get("effects")
        .and_then(|effects| effects.as_array())
        .map(|rows| {
            rows.iter()
                .filter(|row| {
                    row.get("provenance")
                        .and_then(|provenance| provenance.get("type"))
                        .and_then(|value| value.as_str())
                        == Some("evidence")
                })
                .count()
        })
        .unwrap_or(0);
    // Envelope cross-check: the document's own metadata must describe
    // exactly the rows it carries and the typed decoder must accept
    // exactly those rows (a spliced/edited snapshot refuses).
    let metadata = value
        .get("metadata")
        .ok_or_else(|| producer_invalid("producer-shape", "effects"))?;
    let metadata_count = |key: &str| -> Option<usize> {
        metadata
            .get(key)
            .and_then(|value| value.as_u64())
            .map(|count| count as usize)
    };
    let rows = value
        .get("effects")
        .and_then(|effects| effects.as_array())
        .map(|rows| rows.len())
        .unwrap_or(0);
    if metadata_count("detectedCount") != Some(detected)
        || metadata_count("declaredCount") != Some(declared)
        || metadata_count("effectCount") != Some(rows)
        || detected_rows.len() != detected
    {
        return Err(producer_invalid("producer-shape", "effects"));
    }
    let project = value
        .get("project")
        .and_then(|value| value.as_str())
        .ok_or_else(|| producer_invalid("producer-shape", "effects"))?;
    Ok(EffectsCustody {
        project: project.to_owned(),
    })
}

/// Decode the effects snapshot's evidence rows through the published
/// #14 typed constructors: operation, kind, resource/field subject,
/// occurrence, trust state, and the envelope identity fields are each
/// grammar-checked by their owners, never hand-picked here.
fn decode_effects_rows(
    value: &serde_json::Value,
) -> Result<Vec<lekalo_core::effects::EvidenceEntry>, DomainResult> {
    let rows = value
        .get("effects")
        .and_then(|effects| effects.as_array())
        .ok_or_else(|| producer_invalid("producer-shape", "effects"))?;
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        let provenance = row
            .get("provenance")
            .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
        if provenance.get("type").and_then(|value| value.as_str()) != Some("evidence") {
            continue;
        }
        let operation = row
            .get("operation")
            .and_then(|value| value.as_str())
            .and_then(lekalo_core::effects::OperationId::from_qualified)
            .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
        let kind = row
            .get("kind")
            .and_then(|value| value.as_str())
            .and_then(effects_kind_from_key)
            .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
        let resource_id = row
            .get("resource")
            .and_then(|resource| resource.get("id"))
            .and_then(|value| value.as_str())
            .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
        let resource_kind = row
            .get("resource")
            .and_then(|resource| resource.get("kind"))
            .and_then(|value| value.as_str())
            .and_then(effects_resource_kind_from_key)
            .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
        let resource = lekalo_core::effects::ResourceId::new(resource_kind, resource_id)
            .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
        let subject = match row.get("field").and_then(|value| value.as_str()) {
            Some(field) => lekalo_core::effects::Subject::with_field(
                resource,
                lekalo_core::effects::FieldName::new(field)
                    .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?,
            ),
            None => lekalo_core::effects::Subject::new(resource),
        };
        let occurrence = row
            .get("occurrence")
            .and_then(|value| value.as_u64())
            .filter(|occurrence| *occurrence <= u32::MAX as u64)
            .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?
            as u32;
        let trust = row
            .get("provenance")
            .and_then(|provenance| provenance.get("trust"))
            .and_then(|value| value.as_str())
            .and_then(lekalo_core::effects::TrustState::from_key)
            .filter(|trust| *trust != lekalo_core::effects::TrustState::Rejected)
            .ok_or_else(|| producer_invalid("producer-malformed", "effects"))?;
        // The envelope identity fields are grammar-checked by the same
        // rules the #14 decoder applies to them (namespaced adapter and
        // target ids, exact protocol semver, sha256 evidence digest).
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
        entries.push(lekalo_core::effects::EvidenceEntry {
            operation,
            kind,
            subject,
            occurrence,
            trust: Some(trust),
            transaction_group: None,
            sensitivity: None,
        });
    }
    Ok(entries)
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

/// Read one admitted input file (bounded).
fn read_admitted(path: &str) -> Result<Vec<u8>, &'static str> {
    let file = std::fs::File::open(path).map_err(|_| "io")?;
    let mut bytes = Vec::new();
    file.take(lekalo_core::search::MAX_POSTINGS as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "io")?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("input-bytes");
    }
    Ok(bytes)
}

/// One producer-custody refusal with a bounded producer reason.
fn producer_invalid(detail: &'static str, producer: &'static str) -> DomainResult {
    DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
        detail,
        Some(producer),
    ))
}

/// The snapshot content digest a fresh build would publish for one
/// project right now: the exact same admitted assembly the build path
/// runs (canonical Model + manifest producers, bounded reads), hashed
/// over its own snapshot document. `Ok(None)` when no manifest exists
/// at the standard location (admission-dependent freshness is then the
/// build path's business, never a silent stale verdict).
fn current_snapshot_digest(
    selection: &lekalo_core::loader::LoadSelection,
) -> Result<Option<String>, ()> {
    let root = lekalo_core::doctor::project_root(selection).map_err(|_| ())?;
    let manifest_path = root.join("evidence").join("search-inputs.json");
    if !manifest_path.is_file() {
        return Ok(None);
    }
    let page = PageArgs {
        limit: None,
        cursor: None,
        project: selection.project.clone(),
        inputs: Some(
            manifest_path
                .strip_prefix(&root)
                .map_err(|_| ())?
                .to_string_lossy()
                .replace('\\', "/"),
        ),
    };
    let service = assemble(&page, false).map_err(|_| ())?;
    let document = snapshot_document(&service);
    Ok(Some(lekalo_core::search::digest_of(&document)))
}

/// Parse every admitted producer document with its own published
/// decoder and project the typed facts onto the shared projection.
///
/// Custody: every producer document must bind the declared project, its
/// published contract identity (when pinned), and its declared bytes
/// digest (when pinned); the read is confined under the project root by
/// a typed canonical-path containment check (belt and braces over the
/// manifest grammar), so no manifest spelling can escape the project.
fn admit_producers(
    selection: &lekalo_core::loader::LoadSelection,
    project_id: Option<&str>,
    manifest: &SearchInputs,
    projection: &mut Projection,
    coverage: &mut Vec<lekalo_core::search::CoverageRow>,
) -> Result<u64, DomainResult> {
    let root = lekalo_core::doctor::project_root(selection)?;
    let canonical_root = std::fs::canonicalize(&root).map_err(|_| {
        DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
            "root-unresolvable",
            None,
        ))
    })?;
    let cap = lekalo_core::search::MAX_POSTINGS;
    let mut states: Vec<(&'static str, &'static str)> = Vec::new();
    let mut dropped_effects: u64 = 0;
    for entry in manifest.entries() {
        // Confined read: the manifest grammar already refuses absolute
        // and traversal spellings; this second barrier refuses any path
        // that resolves (junctions/symlinks included) outside the
        // canonical project root.
        let path = root.join(&entry.path);
        let contained = std::fs::canonicalize(&path)
            .ok()
            .is_some_and(|resolved| resolved.starts_with(&canonical_root));
        if !contained {
            return Err(DomainResult::invalid(
                lekalo_core::search::diagnostic_input_invalid(
                    "path-confinement",
                    Some(entry.kind.key()),
                ),
            ));
        }
        let bytes = std::fs::read(&path).map_err(|_| {
            DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
                "producer-unreadable",
                Some(entry.kind.key()),
            ))
        })?;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err(DomainResult::invalid(
                lekalo_core::search::diagnostic_input_invalid(
                    "producer-bytes",
                    Some(entry.kind.key()),
                ),
            ));
        }
        if let Some(expected) = &entry.digest {
            let actual = format!("sha256:{}", lekalo_core::search::sha256_of(&bytes));
            if &actual != expected {
                return Err(DomainResult::invalid(
                    lekalo_core::search::diagnostic_input_invalid(
                        "digest-mismatch",
                        Some(entry.kind.key()),
                    ),
                ));
            }
        }
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
            }
            lekalo_core::search::ProducerKind::ObservedIndex => {
                let index =
                    lekalo_core::observed::parse_index(&bytes).map_err(DomainResult::invalid)?;
                if let Some(project) = project_id {
                    check_producer_project(&index.project, project)?;
                }
                project_observed(projection, &index, cap);
                states.push(("bindings", "complete"));
            }
            lekalo_core::search::ProducerKind::Effects => {
                let value = producer_json(&bytes, "effects")?;
                let envelope = decode_effects_envelope(&bytes, &value)?;
                if let Some(project) = project_id {
                    check_producer_project(&envelope.project, project)?;
                }
                let omitted = project_effects_document(projection, &value, cap);
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
            Some(_) => {}
            None => coverage.push(coverage_row(domain, state)),
        }
    }
    // Lost candidates are counted, never silent: the caller surfaces a
    // bounded work-cap warning beside the (explicitly partial) answer.
    Ok(dropped_effects)
}
/// Project the neutral trace relations onto requirement/symbol/gate
/// cards with per-relationship trust.
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
    let relations: Vec<(String, String, String)> = manifest
        .relations()
        .map(|relation| {
            (
                relation.relation_kind.as_str().to_owned(),
                relation.from_node.clone(),
                relation.to_node.clone(),
            )
        })
        .collect();
    for (relation_kind, from, to) in relations {
        for (side, other, direction) in [(&from, &to, "outgoing"), (&to, &from, "incoming")] {
            let key = evidence_key_of(side, &|node_id, requirement_id| {
                requirement_key(node_id, requirement_id)
            });
            let Some(card) = projection.cards.get(&key) else {
                continue;
            };
            let mut relations = card.relations.clone();
            relations.push(lekalo_core::search::RelationWitness {
                relation: relation_kind.clone(),
                other: evidence_key_of(other, &|node_id, requirement_id| {
                    requirement_key(node_id, requirement_id)
                }),
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
            if let Some(card) = projection.cards.get_mut(&key) {
                card.relations = relations;
            }
        }
    }
    complete
}

/// The qualified key of one trace node reference, with the
/// requirement-source qualifier resolved by the caller.
fn evidence_key_of(reference: &str, requirement: &dyn Fn(&str, &str) -> String) -> String {
    let (kind, id) = reference.split_once(':').unwrap_or(("symbol", reference));
    match kind {
        "symbol" | "scenario" => format!("def:{id}"),
        "requirement" => requirement(reference, id),
        "gate" => format!("gate:{id}"),
        "binding" => format!("bind:{id}"),
        _ => format!("ref:{reference}"),
    }
}

/// Project the recorded native bindings of one observed index.
fn project_observed(
    projection: &mut Projection,
    index: &lekalo_core::observed::ObservedIndex,
    cap: usize,
) {
    let target = index.target.as_deref().unwrap_or(index.adapter.id.as_str());
    for record in &index.symbols {
        let key = format!("def:{}", record.id);
        // Sensitive/excluded-path policy: recorded native locations are
        // projected only through the frozen privacy path grammar. A
        // location outside the portable project-relative spelling
        // (dotfiles like `.env`, absolute/UNC spellings, traversal
        // segments) is withheld — the card stays visible but carries no
        // sensitive path/symbol bytes, and its completeness discloses
        // the withholding instead of leaking the subject.
        let location_path = record
            .location
            .as_ref()
            .map(|location| location.path.clone())
            .unwrap_or_default();
        let path_admitted = location_path.is_empty()
            || lekalo_core::project_fs::path_violation(&location_path).is_none();
        let stable_key = if path_admitted {
            record
                .stable_key
                .clone()
                .unwrap_or_else(|| record.id.clone())
        } else {
            record.id.clone()
        };
        let admitted_symbol = || {
            record
                .candidates
                .first()
                .map(|candidate| candidate.native.clone())
                .filter(|native| native_path_admitted(native))
        };
        // The binding card exists alongside the definition card so a
        // native lookup finds recorded evidence either way.
        let binding = Card {
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
            bindings: vec![NativeBinding {
                key: stable_key.clone(),
                target: target.to_owned(),
                path: if path_admitted {
                    location_path.clone()
                } else {
                    String::new()
                },
                symbol: admitted_symbol(),
                status: record.status.key().to_owned(),
            }],
            locations: Vec::new(),
            requirements: Vec::new(),
            scenarios: Vec::new(),
            gates: record.gates.clone(),
            portability: None,
            visibility: None,
            confidence: Confidence::Extracted,
            freshness: Freshness::AsRecorded,
            completeness: if path_admitted {
                Completeness::Complete
            } else {
                Completeness::Withheld
            },
            omitted: 0,
            provenance: "observed".to_owned(),
        };
        let binding_key = format!("bind:{}", record.id);
        add_card(projection, binding, cap);
        // Exact qualified identities: the stable binding key, the
        // symbol spelling, and every recorded candidate native spelling
        // all answer the exact-qualified tier for both cards. Sensitive
        // paths never enter the identity set (the tier is byte-exact:
        // an unadmitted spelling is not a legal query identity).
        for identity in [
            path_admitted.then(|| record.stable_key.clone()).flatten(),
            path_admitted.then_some(location_path.clone()),
        ]
        .into_iter()
        .flatten()
        .chain(
            record
                .candidates
                .iter()
                .map(|candidate| candidate.native.clone())
                .filter(|native| native_path_admitted(native)),
        ) {
            lekalo_core::search::add_qualified_id(projection, &binding_key, identity.clone(), cap);
            lekalo_core::search::add_qualified_id(projection, &key, identity, cap);
        }
        // Attach the recorded native path/symbol onto the definition
        // card's exact qualified identities so `--native-symbol` and
        // `--binding-path` resolve through typed identities. Sensitive
        // candidate paths are withheld with the same grammar.
        if let Some(card) = projection.cards.get_mut(&key) {
            let bindings: Vec<NativeBinding> = record
                .candidates
                .iter()
                .filter(|candidate| native_path_admitted(&candidate.path))
                .map(|candidate| NativeBinding {
                    key: candidate.native.clone(),
                    target: target.to_owned(),
                    path: candidate.path.clone(),
                    symbol: Some(candidate.native.clone()),
                    status: "inferred".to_owned(),
                })
                .collect();
            let mut merged = card.bindings.clone();
            for binding in bindings {
                if !merged.contains(&binding) {
                    merged.push(binding);
                }
            }
            merged.sort_by(|left, right| {
                (&left.key[..], &left.path[..]).cmp(&(&right.key[..], &right.path[..]))
            });
            card.bindings = merged;
            if !path_admitted && card.completeness == Completeness::Complete {
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

/// Project the admitted producer's effect rows: the exact detected
/// evidence of the published effect-graph wire (`lekalo/effects/v0.2.16`).
///
/// Only evidence-provenance rows are projected here: the declared rows
/// are rebuilt from the canonical IR at admission, so copying them from
/// a saved snapshot could only ever double-count. Every row keeps its
/// exact subject (resource plus optional field) so an entity-wide
/// effect can never claim a field it did not name, and the row's own
/// recorded trust state is preserved per relationship. Rows that name
/// an operation this projection does not know are recorded as omitted
/// (lost candidates are counted, never silently dropped).
fn project_effects_document(
    projection: &mut Projection,
    document: &serde_json::Value,
    cap: usize,
) -> usize {
    let Some(rows) = document
        .get("effects")
        .and_then(|effects| effects.as_array())
    else {
        return 0;
    };
    let mut witnesses: Vec<(String, lekalo_core::search::RelationWitness)> = Vec::new();
    let mut omitted = 0u32;
    for row in rows {
        let Some(operation) = row.get("operation").and_then(|value| value.as_str()) else {
            omitted += 1;
            continue;
        };
        let Some(semantic) = operation.strip_prefix("operation:") else {
            omitted += 1;
            continue;
        };
        let card_key = format!("def:{semantic}");
        if !projection.cards.contains_key(&card_key) {
            omitted += 1;
            continue;
        }
        let Some(provenance) = row.get("provenance") else {
            omitted += 1;
            continue;
        };
        if provenance.get("type").and_then(|value| value.as_str()) != Some("evidence") {
            continue;
        }
        let Some(kind) = row.get("kind").and_then(|value| value.as_str()) else {
            omitted += 1;
            continue;
        };
        let action = match kind {
            "read" => "reads",
            "create" | "update" | "delete" | "write-field" => "writes",
            _ => continue,
        };
        let Some(resource) = row
            .get("resource")
            .and_then(|value| value.get("id"))
            .and_then(|value| value.as_str())
        else {
            omitted += 1;
            continue;
        };
        let field = row.get("field").and_then(|value| value.as_str());
        let relation = match field {
            Some(field) => format!("{action}:{resource}:{field}"),
            None => format!("{action}:{resource}"),
        };
        let trust_key = provenance
            .get("trust")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let confidence = match trust_key {
            "verified" | "current" => Confidence::Verified,
            "extracted" => Confidence::Extracted,
            "inferred" => Confidence::Inferred,
            _ => Confidence::Unknown,
        };
        let witness = lekalo_core::search::RelationWitness {
            relation,
            other: format!("resource:{resource}"),
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
    omitted as usize
}

/// Project qualified waiver cards from one typed governance waiver
/// store. Active entries are current; revoked/superseded entries stay
/// visibly stale and are excluded unless the request explicitly
/// includes stale rows.
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
            // Freshness: the read-only status re-admits the canonical
            // Model plus manifest inputs with bounded reads, recomputes
            // the exact snapshot content digest a fresh build would
            // publish, and compares it against the stored generation. A
            // changed input, pin, or model makes the generation stale
            // (LEK-SRCH-004). This never repairs and never enumerates
            // source.
            let mut diagnostics: Vec<lekalo_core::diagnostics::Diagnostic> = Vec::new();
            let state = if status.state == "ready" {
                let generation = status.generation.clone().unwrap_or_default();
                match current_snapshot_digest(&selection) {
                    Ok(Some(fresh)) => {
                        if fresh == generation {
                            "ready"
                        } else {
                            diagnostics.extend(
                                lekalo_core::search::emit_index_stale("input-pin")
                                    .as_slice()
                                    .to_vec(),
                            );
                            "stale"
                        }
                    }
                    Ok(None) => "ready",
                    Err(_) => {
                        diagnostics.extend(
                            lekalo_core::search::emit_index_stale("input-admission")
                                .as_slice()
                                .to_vec(),
                        );
                        "stale"
                    }
                }
            } else {
                status.state
            };
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
            let document = snapshot_document(&service);
            let digest = lekalo_core::search::digest_of(&document);
            // Incremental semantics: when the recomputed content digest
            // equals the published generation, update is a no-op
            // republish of that same immutable generation (partition
            // reuse); any changed input rewrites the whole snapshot.
            // Content identity, not a rebuild alias, decides.
            if let Ok(Some(current)) = store.current() {
                if current == digest {
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
