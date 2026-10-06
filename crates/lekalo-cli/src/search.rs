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
/// immutable content digest, and the per-domain admitted coverage.
struct Service {
    projection: Projection,
    project_id: Option<String>,
    coverage: Vec<lekalo_core::search::CoverageRow>,
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
    let service = match assemble(page, no_cache) {
        Ok(service) => service,
        Err(result) => return result,
    };
    let cap = lekalo_core::search::MAX_POSTINGS;
    let index = match lekalo_core::search::SearchIndex::from_projection(
        &service.projection,
        &[],
        &service.policy_digest(),
        service.project_id.as_deref(),
        cap,
    ) {
        Ok(index) => index,
        Err(set) => return DomainResult::invalid(set),
    };
    let document = snapshot_document(&service);
    let index_digest = format!("sha256:{}", lekalo_core::search::digest_of(&document));
    let outcome = match lekalo_core::search::execute(
        &index,
        &index_digest,
        &index_digest,
        service.project_id.as_deref(),
        request,
        service.coverage,
    ) {
        Ok(outcome) => outcome,
        Err(set) => return DomainResult::invalid(set),
    };
    DomainResult::graph(outcome.json, outcome.human, outcome.diagnostics)
}

fn snapshot_document(service: &Service) -> lekalo_core::search::IndexDocument {
    let index = lekalo_core::search::SearchIndex::from_projection(
        &service.projection,
        &[],
        "",
        None,
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
        inputs: Vec::new(),
        policy_digest: String::new(),
        cards,
        postings,
        qualified_ids,
    }
}

impl Service {
    /// The admitted policy digest: the snapshot content itself. Policy
    /// tightening therefore rebinds every cursor coherently.
    fn policy_digest(&self) -> String {
        format!(
            "sha256:{}",
            lekalo_core::search::sha256_of(
                serde_json::to_vec(&self.coverage)
                    .unwrap_or_default()
                    .as_slice()
            )
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
    let (_model, compilation) =
        lekalo_core::cache::load_compiled(&selection, no_cache).map_err(|result| result)?;
    let graph = lekalo_core::effects::build(&compilation.project)
        .map_err(DomainResult::invalid)?;
    let cap = lekalo_core::search::MAX_POSTINGS;
    let mut projection = project_ir(&compilation.project, cap);
    attach_effects(&mut projection, &graph, cap);
    let mut coverage: Vec<lekalo_core::search::CoverageRow> = vec![
        coverage_row("ir", "complete"),
        coverage_row("effects", "complete"),
    ];
    match &page.inputs {
        Some(path) => {
            let bytes = read_admitted(path).map_err(|detail| {
                DomainResult::invalid(lekalo_core::search::diagnostic_query_invalid(
                    "inputs",
                    detail,
                ))
            })?;
            let manifest = SearchInputs::parse(&bytes).map_err(DomainResult::invalid)?;
            if let Some(project) = &compilation.project.project {
                if manifest.project_id() != project.id.as_str() {
                    return Err(DomainResult::invalid(
                        lekalo_core::search::diagnostic_input_invalid(
                            "project-mismatch",
                            None,
                        ),
                    ));
                }
            }
            admit_producers(&selection, &manifest, &mut projection, &mut coverage)?;
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
    Ok(Service {
        projection,
        project_id: compilation
            .project
            .project
            .as_ref()
            .map(|project| project.id.as_str().to_owned()),
        coverage,
    })
}

fn coverage_row(
    domain: &'static str,
    state: &'static str,
) -> lekalo_core::search::CoverageRow {
    lekalo_core::search::CoverageRow {
        domain: domain.to_owned(),
        state: state.to_owned(),
    }
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

/// Parse every admitted producer document with its own published
/// decoder and project the typed facts onto the shared projection.
fn admit_producers(
    selection: &lekalo_core::loader::LoadSelection,
    manifest: &SearchInputs,
    projection: &mut Projection,
    coverage: &mut Vec<lekalo_core::search::CoverageRow>,
) -> Result<(), DomainResult> {
    let root = match lekalo_core::doctor::project_root(selection) {
        Ok(root) => root,
        Err(result) => return Err(result),
    };
    let cap = lekalo_core::search::MAX_POSTINGS;
    let mut states: Vec<(&'static str, &'static str)> = Vec::new();
    for entry in manifest.entries() {
        let bytes = std::fs::read(root.join(&entry.path)).map_err(|_| {
            DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
                "producer-unreadable",
                Some(entry.kind.key()),
            ))
        })?;
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
                let complete = project_trace(projection, &document, cap);
                states.push(("trace", if complete { "complete" } else { "partial" }));
            }
            lekalo_core::search::ProducerKind::ObservedIndex => {
                let index = lekalo_core::observed::parse_index(&bytes)
                    .map_err(DomainResult::invalid)?;
                project_observed(projection, &index, cap);
                states.push(("bindings", "complete"));
            }
            lekalo_core::search::ProducerKind::Effects => {
                let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
                    DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
                        "producer-malformed",
                        Some("effects"),
                    ))
                })?;
                project_effects_document(projection, &value);
                states.push(("effects", "complete"));
            }
            lekalo_core::search::ProducerKind::Waivers => {
                project_waivers(projection, &bytes, cap)?;
                states.push(("waivers", "complete"));
            }
        }
    }
    // One row per admitted domain: the worst observed state wins, so a
    // partial producer never advertises complete coverage.
    for (domain, state) in states {
        let existing = coverage
            .iter_mut()
            .find(|row| row.domain == domain);
        match existing {
            Some(row) if row.state == "complete" && state != "complete" => row.state = state.to_owned(),
            Some(_) => {}
            None => coverage.push(coverage_row(domain, state)),
        }
    }
    Ok(())
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
                    node.requirement_id
                        .as_deref()
                        .unwrap_or(&node.node_id),
                ),
                node.requirement_id.clone().unwrap_or_else(|| node.node_id.clone()),
            ),
            lekalo_core::trace::NodeKind::Gate => (
                CardKind::Gate,
                format!(
                    "gate:{}",
                    node.gate_id.as_deref().unwrap_or(&node.node_id)
                ),
                node.gate_id.clone().unwrap_or_else(|| node.node_id.clone()),
            ),
            lekalo_core::trace::NodeKind::Scenario => (
                CardKind::Scenario,
                format!(
                    "def:{}",
                    node.scenario_id.as_deref().unwrap_or(&node.node_id)
                ),
                node.scenario_id.clone().unwrap_or_else(|| node.node_id.clone()),
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
        for (side, other, direction) in [
            (&from, &to, "outgoing"),
            (&to, &from, "incoming"),
        ] {
            let key = evidence_key_of(
                side,
                &|node_id, requirement_id| requirement_key(node_id, requirement_id),
            );
            let Some(card) = projection.cards.get(&key) else {
                continue;
            };
            let mut relations = card.relations.clone();
            relations.push(lekalo_core::search::RelationWitness {
                relation: relation_kind.clone(),
                other: evidence_key_of(
                    other,
                    &|node_id, requirement_id| requirement_key(node_id, requirement_id),
                ),
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
fn evidence_key_of(
    reference: &str,
    requirement: &dyn Fn(&str, &str) -> String,
) -> String {
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
        // The binding card exists alongside the definition card so a
        // native lookup finds recorded evidence either way.
        let binding = Card {
            key: format!("bind:{}", record.id),
            kind: CardKind::Binding,
            subkind: Some(record.kind.key().to_owned()),
            module: record.id.split_once('.').map(|(module, _)| module.to_owned()),
            id: Some(record.id.clone()),
            name: Some(record.id.clone()),
            description: None,
            signatures: Vec::new(),
            relations: Vec::new(),
            effects: Vec::new(),
            bindings: vec![NativeBinding {
                key: record
                    .stable_key
                    .clone()
                    .unwrap_or_else(|| record.id.clone()),
                target: target.to_owned(),
                path: record
                    .location
                    .as_ref()
                    .map(|location| location.path.clone())
                    .unwrap_or_default(),
                symbol: record.candidates.first().map(|candidate| candidate.native.clone()),
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
            completeness: Completeness::Complete,
            omitted: 0,
            provenance: "observed".to_owned(),
        };
        add_card(projection, binding, cap);
        // Attach the recorded native path/symbol onto the definition
        // card's exact qualified identities so `--native-symbol` and
        // `--binding-path` resolve through typed identities.
        if let Some(card) = projection.cards.get_mut(&key) {
            let bindings: Vec<NativeBinding> = record
                .candidates
                .iter()
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
        }
    }
}

/// Project the admitted producer's effect rows: the exact detected
/// evidence of the published effect-graph wire (`lekalo/effects/v0.2.16`).
///
/// Only evidence-provenance rows are projected here: the declared rows
/// are rebuilt from the canonical IR at admission, so copying them from
/// a saved snapshot could only ever double-count. Every row keeps its
/// exact subject (resource plus optional field) so an entity-wide
/// effect can never claim a field it did not name, and the row's own
/// recorded trust state is preserved per relationship.
fn project_effects_document(
    projection: &mut Projection,
    document: &serde_json::Value,
) {
    let Some(rows) = document.get("effects").and_then(|effects| effects.as_array()) else {
        return;
    };
    let mut witnesses: Vec<(String, lekalo_core::search::RelationWitness)> = Vec::new();
    for row in rows {
        let Some(operation) = row.get("operation").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(semantic) = operation.strip_prefix("operation:") else {
            continue;
        };
        let card_key = format!("def:{semantic}");
        if !projection.cards.contains_key(&card_key) {
            continue;
        }
        let Some(provenance) = row.get("provenance") else {
            continue;
        };
        if provenance.get("type").and_then(|value| value.as_str()) != Some("evidence") {
            continue;
        }
        let Some(kind) = row.get("kind").and_then(|value| value.as_str()) else {
            continue;
        };
        let action = match kind {
            "read" => "reads",
            "create" | "update" | "delete" | "write-field" => "writes",
            _ => continue,
        };
        let Some(resource) = row.get("resource").and_then(|value| value.get("id")).and_then(|value| value.as_str()) else {
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
            }
        }
    }
}

/// Project qualified waiver cards from one AI-lint waiver set. Active
/// entries are current; revoked/superseded entries stay visibly stale
/// and are excluded unless the request explicitly includes stale rows.
fn project_waivers(
    projection: &mut Projection,
    bytes: &[u8],
    cap: usize,
) -> Result<(), DomainResult> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| {
        DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
            "producer-malformed",
            Some("waivers"),
        ))
    })?;
    let entries = value
        .get("entries")
        .and_then(|entries| entries.as_array())
        .ok_or_else(|| {
            DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
                "producer-shape",
                Some("waivers"),
            ))
        })?;
    for entry in entries {
        let Some(id) = entry.get("id").and_then(|id| id.as_str()) else {
            continue;
        };
        let lifecycle = entry
            .get("lifecycle")
            .and_then(|value| value.as_str())
            .unwrap_or("active");
        let subject = entry.get("subject").and_then(|value| value.as_str());
        let rule = entry
            .get("selector")
            .and_then(|selector| selector.get("id"))
            .and_then(|value| value.as_str());
        let freshness = if lifecycle == "active" {
            Freshness::Current
        } else {
            Freshness::Stale
        };
        let mut card = Card {
            key: format!("waiv:{id}"),
            kind: CardKind::Waiver,
            subkind: Some(lifecycle.to_owned()),
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
                    return DomainResult::invalid(
                        lekalo_core::search::diagnostic_input_invalid("store-denied", None)
                    )
                }
            };
            let status = store.status();
            let json = format!(
                "{{\"status\":\"valid\",\"searchIndex\":{{\"state\":\"{}\",\"generation\":{},\"cards\":{},\"postings\":{}}}}}",
                status.state,
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
            let human = format!("search index {} : {}", status.state, status.generation.clone().unwrap_or_default());
            DomainResult::graph(json, human, Vec::new())
        }
        SearchIndexCommands::Build { page } | SearchIndexCommands::Update { page } => {
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
                    return DomainResult::invalid(
                        lekalo_core::search::diagnostic_input_invalid("store-denied", None)
                    )
                }
            };
            let service = match assemble(&page, no_cache) {
                Ok(service) => service,
                Err(result) => return result,
            };
            let document = snapshot_document(&service);
            let digest = lekalo_core::search::digest_of(&document);
            let mut document = document;
            document.digest = format!("sha256:{digest}");
            if store.publish(&document).is_err() {
                return DomainResult::invalid(lekalo_core::search::diagnostic_input_invalid(
                    "publish-failed",
                    None,
                ));
            }
            let json = format!(
                "{{\"status\":\"valid\",\"operation\":\"search-index-build\",\"digest\":\"sha256:{digest}\",\"cards\":{}}}",
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
