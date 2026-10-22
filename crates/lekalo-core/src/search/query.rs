//! Query admission, exact filters, lexical tiering, pagination, and the
//! closed `lekalo/search-result/v0.8.0` wire (issue #93).
//!
//! Pipeline, in the frozen order:
//! 1. Admit the typed request (closed grammar; bounds before allocation).
//! 2. Apply admission/privacy and exact filters first (`find` never
//!    fuzzes a selector; repeated values of one flag OR, different
//!    flags AND).
//! 3. Rank the admitted set by the fixed `semantic-search/1` tuple:
//!    exact full ID, exact qualified ID, exact normalized name, all
//!    exact terms, prefixes, optional fuzzy — then matched-field
//!    priority, coverage, edits, and the qualified key in UTF-8 byte
//!    order. Ties inside the final key set are `ambiguous` and retain
//!    every tied candidate.
//! 4. Page the ranked visible set; concatenated pages equal the full
//!    ranked set with no repeats or skips.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::search::cursor::{CursorBinding, CursorError, LastTuple};
use crate::search::diagnostic as emit;
use crate::search::index::SearchIndex;
use crate::search::projection::{Card, CardKind, RelationWitness};
use crate::search::rank::{
    match_facts, normalize_text, tokenize, FieldPriority, FuzzyBudget, RankFacts, Tier,
};
use crate::search::version::{
    is_sha256, DEFAULT_LIMIT, MAX_CURSOR_BYTES, MAX_LIMIT, MAX_QUERY_SCALARS, MAX_QUERY_TERMS,
    MAX_RESPONSE_BYTES, RECIPE, RESULT_IDENTITY, RESULT_SCHEMA_VERSION,
};
use crate::search::wire::{canonical_json, sha256_digest};
use crate::search::Cursor as PageCursor;

/// The closed effect-scope selector (`--effect-scope`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum EffectScope {
    /// Only exact-field effects admit.
    #[default]
    Exact,
    /// Exact-field plus explicitly labeled entity-wide overlap.
    Overlapping,
}

impl EffectScope {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Overlapping => "overlapping",
        }
    }
}

/// The closed exact filter set (`find`'s whole surface; `search` applies
/// the same filters before any lexical work).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FilterSet {
    pub ids: Vec<String>,
    pub kinds: Vec<String>,
    pub modules: Vec<String>,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub calls: Vec<String>,
    pub requirements: Vec<String>,
    pub requirement_sources: Vec<String>,
    pub binding_paths: Vec<String>,
    pub native_symbols: Vec<String>,
    pub targets: Vec<String>,
    pub types: Vec<String>,
    pub input_types: Vec<String>,
    pub output_types: Vec<String>,
    pub errors: Vec<String>,
    pub relations: Vec<String>,
    pub related_to: Vec<String>,
    pub ownerships: Vec<String>,
    pub portabilities: Vec<String>,
    pub profiles: Vec<String>,
    pub diagnostics: Vec<String>,
    pub waivers: Vec<String>,
    pub include_stale: bool,
    pub effect_scope: EffectScope,
    /// Whether the caller explicitly passed `--effect-scope`: the
    /// documented grammar makes the flag legal with reads/writes only,
    /// and presence (not just value) decides — the default value must
    /// never launder an illegal explicit argument.
    pub effect_scope_present: bool,
}

impl FilterSet {
    /// Whether any exact filter is present (find requires one).
    pub fn any(&self) -> bool {
        !self.ids.is_empty()
            || !self.kinds.is_empty()
            || !self.modules.is_empty()
            || !self.reads.is_empty()
            || !self.writes.is_empty()
            || !self.calls.is_empty()
            || !self.requirements.is_empty()
            || !self.requirement_sources.is_empty()
            || !self.binding_paths.is_empty()
            || !self.native_symbols.is_empty()
            || !self.targets.is_empty()
            || !self.types.is_empty()
            || !self.input_types.is_empty()
            || !self.output_types.is_empty()
            || !self.errors.is_empty()
            || !self.relations.is_empty()
            || !self.related_to.is_empty()
            || !self.ownerships.is_empty()
            || !self.portabilities.is_empty()
            || !self.profiles.is_empty()
            || !self.diagnostics.is_empty()
            || !self.waivers.is_empty()
    }

    /// The filter digest member of the cursor binding: every member in
    /// closed order, so any change rebinds the page.
    fn canonical_members(&self) -> String {
        format!(
            "effect-scope={};ids={};include-stale={};input-types={};kinds={};modules={};native-symbols={};ownerships={};output-types={};profiles={};portabilities={};reads={};related-to={};relations={};requirement-sources={};requirements={};targets={};types={};waivers={};writes={};binding-paths={};calls={};diagnostics={};errors={}",
            self.effect_scope.key(),
            self.ids.join(","),
            self.include_stale,
            self.input_types.join(","),
            self.kinds.join(","),
            self.modules.join(","),
            self.native_symbols.join(","),
            self.ownerships.join(","),
            self.output_types.join(","),
            self.profiles.join(","),
            self.portabilities.join(","),
            self.reads.join(","),
            self.related_to.join(","),
            self.relations.join(","),
            self.requirement_sources.join(","),
            self.requirements.join(","),
            self.targets.join(","),
            self.types.join(","),
            self.waivers.join(","),
            self.writes.join(","),
            self.binding_paths.join(","),
            self.calls.join(","),
            self.diagnostics.join(","),
            self.errors.join(","),
        )
    }
}

/// The typed search/find request after CLI admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryRequest {
    /// The lexical query (`search` only; `find` runs filters alone).
    pub query: Option<String>,
    /// Opt-in bounded fuzzy (labeled `lexical-fuzzy`).
    pub fuzzy: bool,
    /// The page size.
    pub limit: usize,
    /// The opaque continuation token.
    pub cursor: Option<String>,
    /// The exact filters.
    pub filters: FilterSet,
}

impl QueryRequest {
    /// Admit one request: closed grammar and bounds before allocation.
    pub fn admit(
        query: Option<String>,
        fuzzy: bool,
        limit: Option<usize>,
        cursor: Option<String>,
        filters: FilterSet,
    ) -> Result<Self, crate::diagnostics::DiagnosticSet> {
        if let Some(query) = &query {
            let scalars = query.chars().count();
            if scalars == 0 || scalars > MAX_QUERY_SCALARS {
                return Err(emit::query_invalid("query", "length-bounds"));
            }
            let terms = tokenize(&normalize_text(query));
            if terms.len() > MAX_QUERY_TERMS {
                return Err(emit::query_invalid("query", "term-bounds"));
            }
        } else if !filters.any() {
            return Err(emit::query_invalid("find", "filter-required"));
        }
        let limit = limit.unwrap_or(DEFAULT_LIMIT);
        if limit == 0 || limit > MAX_LIMIT {
            return Err(emit::query_invalid("limit", "range"));
        }
        if let Some(cursor) = &cursor {
            if cursor.is_empty() || cursor.len() > MAX_CURSOR_BYTES {
                return Err(emit::cursor_invalid("bounds"));
            }
        }
        // The closed runtime filter grammar: the same per-flag member
        // caps and value patterns the published `search-query` schema
        // declares (maxItems, uniqueItems, minLength/maxLength, and the
        // closed per-member value patterns) are enforced here so
        // in-process consumers share the CLI's closed grammar exactly.
        enforce_filter_grammar(&filters)?;
        for kind in &filters.kinds {
            if CardKind::parse(kind).is_none() {
                return Err(emit::query_invalid("kind", "unknown-kind"));
            }
        }
        if filters.relations.is_empty() != filters.related_to.is_empty() {
            return Err(emit::query_invalid("related-to", "relation-pair"));
        }
        if (filters.effect_scope_present || filters.effect_scope != EffectScope::Exact)
            && filters.reads.is_empty()
            && filters.writes.is_empty()
        {
            return Err(emit::query_invalid("effect-scope", "reads-writes-only"));
        }
        Ok(Self {
            query,
            fuzzy,
            limit,
            cursor,
            filters,
        })
    }
}

/// The closed runtime filter grammar, mirroring the published
/// `search-query` schema member-for-member: per-flag item caps,
/// uniqueness where the schema demands it, and the exact per-member
/// value bounds/patterns. A violation is `search.query-invalid` with
/// the offending member named (never its value).
fn enforce_filter_grammar(filters: &FilterSet) -> Result<(), crate::diagnostics::DiagnosticSet> {
    // `^[A-Za-z0-9_][A-Za-z0-9_.:-]*$`, 1..=192.
    let semantic_id = |value: &str| {
        is_pattern_value(value, 192, |first| {
            first.is_ascii_alphanumeric() || first == '_'
        }) && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | ':' | '-')
        })
    };
    // `^[A-Za-z0-9_][A-Za-z0-9_.:/#-]*$`, 1..=192.
    let selector = |value: &str| {
        is_pattern_value(value, 192, |first| {
            first.is_ascii_alphanumeric() || first == '_'
        }) && value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '_' | '.' | ':' | '/' | '#' | '-')
        })
    };
    // `^[A-Za-z0-9._-][A-Za-z0-9._/-]*$`, 1..=512.
    let logical_path = |value: &str| {
        is_pattern_value(value, 512, |first| {
            first.is_ascii_alphanumeric() || matches!(first, '.' | '_' | '-')
        }) && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '/' | '-')
        })
    };
    // `^[a-z][a-z0-9-]*$`, 1..=64.
    let source = |value: &str| {
        is_pattern_value(value, 64, |first| first.is_ascii_lowercase())
            && value.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
    };
    // `^[A-Za-z0-9][A-Za-z0-9_.:/#@-]*$`, 1..=256.
    let native_symbol = |value: &str| {
        is_pattern_value(value, 256, |first| first.is_ascii_alphanumeric())
            && value.chars().all(|character| {
                character.is_ascii_alphanumeric()
                    || matches!(character, '_' | '.' | ':' | '/' | '#' | '@' | '-')
            })
    };
    // `^[A-Za-z0-9][A-Za-z0-9_.:/#@\\-]*$`, 1..=256.
    let waiver = |value: &str| {
        is_pattern_value(value, 256, |first| first.is_ascii_alphanumeric())
            && value.chars().all(|character| {
                character.is_ascii_alphanumeric()
                    || matches!(character, '_' | '.' | ':' | '/' | '#' | '@' | '\\' | '-')
            })
    };
    // `^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$`, 3..=128.
    let diagnostic = |value: &str| {
        (3..=128).contains(&value.len())
            && value.split('.').all(|segment| {
                let mut chars = segment.chars();
                match chars.next() {
                    Some(first) if first.is_ascii_lowercase() => {}
                    _ => return false,
                }
                segment.len() <= 64
                    && segment.chars().all(|character| {
                        character.is_ascii_lowercase()
                            || character.is_ascii_digit()
                            || character == '-'
                    })
                    && segment
                        .ends_with(|last: char| last.is_ascii_lowercase() || last.is_ascii_digit())
            })
            && value.contains('.')
            && value.split('.').count() >= 2
    };
    let bounded = |member: &str,
                   values: &[String],
                   cap: usize,
                   unique: bool,
                   valid: &dyn Fn(&str) -> bool| {
        if values.len() > cap {
            return Err(emit::query_invalid(member, "item-bounds"));
        }
        if unique
            && values.len()
                != values
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
        {
            return Err(emit::query_invalid(member, "duplicate-value"));
        }
        for value in values {
            if !valid(value) {
                return Err(emit::query_invalid(member, "value-grammar"));
            }
        }
        Ok(())
    };
    bounded("ids", &filters.ids, 32, false, &semantic_id)?;
    bounded("kind", &filters.kinds, 32, true, &|value| {
        CardKind::parse(value).is_some()
    })?;
    bounded("module", &filters.modules, 32, false, &semantic_id)?;
    bounded("reads", &filters.reads, 32, false, &selector)?;
    bounded("writes", &filters.writes, 32, false, &selector)?;
    bounded("calls", &filters.calls, 32, false, &semantic_id)?;
    bounded("requirement", &filters.requirements, 32, false, &selector)?;
    bounded(
        "requirement-source",
        &filters.requirement_sources,
        8,
        false,
        &source,
    )?;
    bounded(
        "binding-path",
        &filters.binding_paths,
        32,
        false,
        &logical_path,
    )?;
    bounded(
        "native-symbol",
        &filters.native_symbols,
        32,
        false,
        &native_symbol,
    )?;
    bounded("target", &filters.targets, 16, false, &selector)?;
    bounded("type", &filters.types, 32, false, &semantic_id)?;
    bounded("input-type", &filters.input_types, 32, false, &semantic_id)?;
    bounded(
        "output-type",
        &filters.output_types,
        32,
        false,
        &semantic_id,
    )?;
    bounded("error", &filters.errors, 32, false, &semantic_id)?;
    bounded("relation", &filters.relations, 32, true, &|value| {
        matches!(
            value,
            "module-import"
                | "entity-field"
                | "value-object-field"
                | "event-payload"
                | "command-effect"
                | "effect-entity"
                | "scenario-covers"
                | "command-input"
                | "query-input"
                | "query-returns"
                | "command-returns"
                | "query-reads"
                | "policy-applies-to"
                | "endpoint-invokes"
                | "effect-emits"
                | "derived-from"
                | "implements"
                | "binds"
                | "covers"
                | "verifies"
                | "evidences"
                | "binds-target"
        )
    })?;
    bounded("related-to", &filters.related_to, 32, false, &selector)?;
    bounded("ownership", &filters.ownerships, 8, true, &|value| {
        matches!(value, "module" | "project" | "observed")
    })?;
    bounded("portability", &filters.portabilities, 8, true, &|value| {
        matches!(value, "portable" | "target-specific")
    })?;
    bounded("profile", &filters.profiles, 16, false, &selector)?;
    bounded("diagnostic", &filters.diagnostics, 32, false, &diagnostic)?;
    bounded("waiver", &filters.waivers, 32, false, &waiver)?;
    Ok(())
}

/// The shared shape half of every schema value pattern: nonempty, byte
/// bounded, and first-character checked (the per-member closures add
/// the character-class half).
fn is_pattern_value(value: &str, max_bytes: usize, first: impl Fn(char) -> bool) -> bool {
    let mut characters = value.chars();
    match characters.next() {
        Some(character) if first(character) => {}
        _ => return false,
    }
    value.len() <= max_bytes
}

/// Whether one producer/domain is needed by this request: a needed
/// domain that is not complete is warned about, so absence is never
/// read as proof, while domains the request does not touch stay quiet.
/// Every request needs the canonical assembly (`ir`, `effects`); the
/// exact filters and kinds name the producer domains they join.
pub fn domain_needed(domain: &str, request: &QueryRequest) -> bool {
    let filters = &request.filters;
    match domain {
        "ir" => true,
        // The effects domain is the declared+detected effect evidence:
        // it is needed exactly when the request asks effect questions
        // (reads/writes selectors). An ordinary id/kind/module query
        // never depends on effect coverage, so its `unknown` state
        // stays quiet and cannot degrade an otherwise complete answer.
        "effects" => !filters.reads.is_empty() || !filters.writes.is_empty(),
        "trace" => {
            !filters.requirements.is_empty()
                || !filters.requirement_sources.is_empty()
                || !filters.relations.is_empty()
                || filters
                    .kinds
                    .iter()
                    .any(|kind| matches!(kind.as_str(), "requirement" | "gate" | "scenario"))
        }
        "requirements" => {
            !filters.requirements.is_empty()
                || !filters.requirement_sources.is_empty()
                || filters.kinds.iter().any(|kind| kind == "requirement")
        }
        "bindings" => {
            !filters.binding_paths.is_empty()
                || !filters.native_symbols.is_empty()
                || !filters.targets.is_empty()
                || filters.kinds.iter().any(|kind| kind == "binding")
        }
        "diagnostics" => {
            !filters.diagnostics.is_empty() || filters.kinds.iter().any(|kind| kind == "diagnostic")
        }
        "waivers" => {
            !filters.waivers.is_empty() || filters.kinds.iter().any(|kind| kind == "waiver")
        }
        // The error-contract and ownership/profile producer domains:
        // their selectors need the admitted producer evidence, so a
        // missing domain is disclosed instead of answering a confident
        // zero over absent coverage.
        "errors" => !filters.errors.is_empty(),
        "profiles" => {
            !filters.profiles.is_empty()
                || !filters.ownerships.is_empty()
                || filters
                    .kinds
                    .iter()
                    .any(|kind| matches!(kind.as_str(), "profile"))
        }
        _ => false,
    }
}

/// Whether the request's relevant producer coverage can establish the
/// `unique` verdict: every domain the request actually needs must be
/// admitted complete. A selector over a partial or unknown domain can
/// never claim a singleton answer — absence of admitted evidence is
/// not proof of absence.
fn coverage_establishes_unique(request: &QueryRequest, coverage: &[CoverageRow]) -> bool {
    let mut needed = false;
    for row in coverage {
        if domain_needed(&row.domain, request) {
            needed = true;
            if row.state != "complete" {
                return false;
            }
        }
    }
    let _ = needed;
    true
}

/// One result item: the bounded selection card plus its match/explanation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Item {
    pub card: Card,
    /// The matched fields, exactness tier, and token/edit facts.
    pub r#match: MatchFacts,
}

/// The closed match explanation of one item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MatchFacts {
    /// The closed tier (`exact-id` | ... | `fuzzy`).
    pub tier: String,
    /// The best matched field (`id` | `name` | `description` |
    /// `filter` for pure filter matches).
    pub field: String,
    /// How many query terms matched exactly.
    pub exact_terms: u32,
    /// How many query terms the query carried.
    pub total_terms: u32,
    /// Total edit operations (fuzzy only; otherwise 0).
    pub edits: u32,
    /// The `lexical-fuzzy` label, present only in fuzzy mode hits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// The per-domain coverage row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CoverageRow {
    /// The allowlisted domain (`ir` | `effects` | `trace` |
    /// `requirements` | `bindings` | `diagnostics` | `waivers`).
    pub domain: String,
    /// `complete` | `partial` | `unknown` | `unsupported` | `withheld`.
    pub state: String,
}

/// The closed result payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SearchResult {
    pub schema_version: String,
    pub identity: String,
    /// The project semantic id, when known.
    pub project_id: Option<String>,
    /// The versioned recipe (`semantic-search/1`).
    pub recipe: String,
    /// The normalized query (empty for pure filter runs).
    pub query: String,
    /// The immutable index content digest the answer was computed over.
    pub index_digest: String,
    /// `none` | `unique` | `multiple` | `ambiguous`.
    pub selection: &'static str,
    /// Per-producer/domain coverage (unknown is never zero).
    pub coverage: Vec<CoverageRow>,
    /// The page items in ranked order.
    pub items: Vec<Item>,
    /// The page state.
    pub page: Page,
}

/// The closed page state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Page {
    pub limit: u32,
    /// How many rows this page returned.
    pub returned: u32,
    /// The exact visible total (only exact for a fully computed set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible_total: Option<u32>,
    /// The next opaque cursor, when more rows exist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    /// Explicit truncation state (work-limit, never silent).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// The finished outcome: exact response bytes plus the non-invalidating
/// diagnostics the run recorded (warnings/info).
#[derive(Debug)]
pub struct SearchOutcome {
    pub result: SearchResult,
    pub json: String,
    pub human: String,
    pub diagnostics: Vec<crate::diagnostics::Diagnostic>,
}

/// The policy digest input of cursor bindings: the admitted security
/// facts (classification/exclusion policy) bound into every page.
pub type PolicyDigest = String;

/// Rank one admitted request over one index and produce the page.
///
/// `policy_digest` binds the page to the admitted policy facts;
/// `coverage` reports the per-domain admitted state of the inputs.
pub fn execute(
    index: &SearchIndex,
    index_digest: &str,
    policy_digest: &str,
    project_id: Option<&str>,
    request: &QueryRequest,
    coverage: Vec<CoverageRow>,
) -> Result<SearchOutcome, crate::diagnostics::DiagnosticSet> {
    let query_text = request.query.as_deref().unwrap_or_default();
    let normalized = if query_text.is_empty() {
        String::new()
    } else {
        normalize_text(query_text)
    };
    let terms = tokenize(&normalized);
    // The cursor binding: recipe + mode + fuzzy + query (normalized AND
    // raw — the raw bytes decide the exact-identity tiers) + every
    // filter member. Any semantic change rebinds the page: a cursor
    // never straddles a mode toggle, a fuzzy toggle, or an edit that
    // would change the candidate set or its ranks.
    let query_digest_input = format!(
        "recipe={};fuzzy={};query={};raw={};filters={}",
        RECIPE,
        request.fuzzy,
        normalized,
        query_text,
        request.filters.canonical_members()
    );
    let query_digest = format!("sha256:{}", sha256_digest(query_digest_input.as_bytes()));
    let binding = CursorBinding {
        recipe: RECIPE.to_owned(),
        query_digest,
        policy_digest: policy_digest.to_owned(),
        index_digest: index_digest.to_owned(),
    };
    // Decode a supplied cursor or start a fresh ranked computation.
    let continuation = match &request.cursor {
        Some(token) => match PageCursor::decode(token, &binding) {
            Ok(cursor) => Some(cursor),
            Err(CursorError::Mismatch) => return Err(emit::cursor_invalid("generation-mismatch")),
            Err(CursorError::Malformed) => return Err(emit::cursor_invalid("malformed")),
        },
        None => None,
    };
    // 1. Exact filters first.
    let admitted = filter_cards(index, &request.filters)?;
    // 2. Rank: facts per admitted card. The exact/exact-qualified/
    //    non-fuzzy tiers are computed for EVERY admitted card first,
    //    independently of the fuzzy work budget: a fuzzy work limit can
    //    never suppress an exact match (AC2). Only the additional fuzzy
    //    expansion — cards that admitted no non-fuzzy facts — spends the
    //    bounded budget; an exhausted budget refuses further fuzzy
    //    expansion and is disclosed as truncation, never silently paid
    //    for with an exact candidate's visibility.
    let mut ranked: Vec<(&Card, RankFacts)> = Vec::with_capacity(admitted.len());
    let mut fuzzy_pending: Vec<&Card> = Vec::new();
    for card in &admitted {
        if query_text.is_empty() {
            ranked.push((
                card,
                RankFacts {
                    tier: Tier::AllTerms,
                    field: FieldPriority::Id,
                    exact_terms: 0,
                    total_terms: 0,
                    edits: 0,
                },
            ));
            continue;
        }
        let qualified = index
            .qualified_ids
            .get(&card.key)
            .cloned()
            .unwrap_or_default();
        match match_facts(
            card.id
                .as_deref()
                .unwrap_or(card.key.trim_start_matches("def:")),
            card.name.as_deref(),
            card.description.as_deref(),
            &qualified,
            query_text,
            &terms,
            false,
        ) {
            Some(facts) => ranked.push((card, facts)),
            // A lexical query never admits non-matching cards; only
            // these unmatched cards are eligible for fuzzy expansion.
            None => fuzzy_pending.push(card),
        }
    }
    let mut fuzzy_budget = FuzzyBudget::new();
    let mut fuzzy_exhausted = false;
    if request.fuzzy {
        for card in fuzzy_pending {
            // The budget is reserved for fuzzy scoring alone: refused
            // expansion is disclosed, never billed against exact work.
            if !fuzzy_budget.take() {
                fuzzy_exhausted = true;
                continue;
            }
            let qualified = index
                .qualified_ids
                .get(&card.key)
                .cloned()
                .unwrap_or_default();
            if let Some(facts) = match_facts(
                card.id
                    .as_deref()
                    .unwrap_or(card.key.trim_start_matches("def:")),
                card.name.as_deref(),
                card.description.as_deref(),
                &qualified,
                query_text,
                &terms,
                true,
            ) {
                ranked.push((card, facts));
            }
        }
    }
    ranked.sort_by(|left, right| {
        left.1
            .sort_key()
            .cmp(&right.1.sort_key())
            .then_with(|| left.0.key.as_bytes().cmp(right.0.key.as_bytes()))
    });
    // 3. Ambiguity: ties inside the TOP rank group (equal tier, field,
    //    coverage, and edits before the final key tie-break) with more
    //    than one distinct key are equally supported interpretations:
    //    the answer is ambiguous and every tied candidate is retained.
    //    A weak tail below a determinate best answer is not an
    //    interpretation, so only the top group decides. Deterministic
    //    sorting never erases pre-sort ambiguity.
    let mut diagnostics: Vec<crate::diagnostics::Diagnostic> = Vec::new();
    let mut top_tied = false;
    if !query_text.is_empty() && ranked.len() > 1 {
        top_tied = ranked[0].1.sort_key() == ranked[1].1.sort_key();
    }
    // The selection verdict over the whole visible set (not the page).
    // A `--requirement` selector without `--requirement-source` that
    // names several equally supported provider-qualified identities is
    // ambiguous: every qualified candidate stays visible, none is taken.
    let mut requirement_ambiguous = false;
    if !request.filters.requirements.is_empty() && request.filters.requirement_sources.is_empty() {
        let mut qualified: BTreeSet<&str> = BTreeSet::new();
        for (card, _) in &ranked {
            if card.kind == CardKind::Requirement {
                qualified.insert(card.key.as_str());
            }
        }
        if qualified.len() > 1 {
            requirement_ambiguous = true;
        }
    }
    let selection = if ranked.is_empty() {
        "none"
    } else if top_tied || requirement_ambiguous {
        "ambiguous"
    } else if ranked.len() == 1 && coverage_establishes_unique(request, &coverage) {
        // An exhausted fuzzy work limit leaves the candidate set
        // incomplete: a singleton over unfinished work is never a
        // confirmed `unique` answer (no exact totals, no uniqueness).
        if fuzzy_exhausted {
            "ambiguous"
        } else {
            "unique"
        }
    } else if ranked.len() == 1 {
        // One hit over coverage that cannot establish uniqueness is an
        // interpretation the admitted evidence cannot confirm: reported
        // ambiguous (with the coverage diagnostics), never unique.
        "ambiguous"
    } else {
        "multiple"
    };
    if selection == "ambiguous" && !ranked.is_empty() && !top_tied && !requirement_ambiguous {
        // The single-candidate coverage downgrade carries the ambiguity
        // explanation too: one candidate, incomplete supporting cover.
        diagnostics.extend(set_items(emit::ambiguous(1)));
    }
    if selection == "ambiguous" {
        let tied = if requirement_ambiguous {
            ranked
                .iter()
                .filter(|(card, _)| card.kind == CardKind::Requirement)
                .count() as u64
        } else {
            ranked
                .iter()
                .take_while(|(_, facts)| facts.sort_key() == ranked[0].1.sort_key())
                .count() as u64
        };
        diagnostics.extend(set_items(emit::ambiguous(tied)));
    }
    if fuzzy_exhausted {
        diagnostics.extend(set_items(emit::result_limit("fuzzy-candidates", false)));
    }
    // 4. Page: resume after the cursor's last tuple.
    let start = match &continuation {
        Some(cursor) => match &cursor.last {
            Some(last) => ranked
                .iter()
                .position(|(card, facts)| {
                    card.key == last.key
                        && facts.tier.rank() == last.tier
                        && facts.field.rank() == last.field
                        && facts.exact_terms == last.coverage
                        && facts.edits == last.edits
                })
                .map_or(ranked.len(), |position| position + 1),
            None => 0,
        },
        None => 0,
    };
    let page_items: Vec<Item> = ranked
        .iter()
        .skip(start)
        .take(request.limit)
        .map(|(card, facts)| Item {
            // The wire card is the bounded response projection: every
            // preview is capped at its published bound with the omitted
            // count disclosed, while the matching/ranking above ran over
            // the complete typed facts. Filtering never sees a
            // truncated fact; the wire never carries an uncapped list.
            card: bounded_response_card(card),
            r#match: MatchFacts {
                tier: facts.tier.as_str().to_owned(),
                field: facts.field.as_str().to_owned(),
                exact_terms: facts.exact_terms as u32,
                total_terms: facts.total_terms as u32,
                edits: facts.edits,
                label: (facts.tier == Tier::Fuzzy).then(|| "lexical-fuzzy".to_owned()),
            },
        })
        .collect();
    let returned = page_items.len();
    let has_more = start + returned < ranked.len();
    let next = if has_more {
        let last = ranked[start + returned - 1].clone();
        let cursor = PageCursor {
            binding,
            last: Some(LastTuple {
                tier: last.1.tier.rank(),
                field: last.1.field.rank(),
                coverage: last.1.exact_terms,
                edits: last.1.edits,
                key: last.0.key.clone(),
            }),
        };
        Some(
            cursor
                .encode()
                .map_err(|_| emit::cursor_invalid("encode"))?,
        )
    } else {
        None
    };
    let result = SearchResult {
        schema_version: RESULT_SCHEMA_VERSION.to_owned(),
        identity: RESULT_IDENTITY.to_owned(),
        project_id: project_id.map(str::to_owned),
        recipe: RECIPE.to_owned(),
        query: normalized,
        index_digest: index_digest.to_owned(),
        selection,
        coverage,
        items: page_items,
        page: Page {
            limit: request.limit as u32,
            returned: returned as u32,
            // An exhausted fuzzy budget means the visible set was not
            // fully computed: no exact total is asserted over unfinished
            // work (the truncation flag carries the honest state).
            visible_total: (!fuzzy_exhausted).then_some(ranked.len() as u32),
            next,
            truncated: fuzzy_exhausted,
        },
    };
    let json = response_bytes(&result)?;
    let human = human_summary(&result);
    Ok(SearchOutcome {
        result,
        json,
        human,
        diagnostics,
    })
}

/// Flatten one non-invalidating set into the valid envelope items.
fn set_items(set: crate::diagnostics::DiagnosticSet) -> Vec<crate::diagnostics::Diagnostic> {
    set.as_slice().to_vec()
}

/// The bounded response projection of one card: every preview list is
/// capped at its published wire bound and the omissions are added to
/// the card's disclosed omitted count. The internal card keeps the
/// complete typed facts for filtering and ranking; only the wire is
/// capped, so a 65th effect or a ninth input can never produce a
/// schema-invalid response or a silent loss.
fn bounded_response_card(card: &Card) -> Card {
    const WIRE_PREVIEWS: usize = 8;
    const WIRE_RELATIONS: usize = 64;
    let mut bounded = card.clone();
    let mut omitted = bounded.omitted as usize;
    omitted += bounded.signatures.len().saturating_sub(WIRE_PREVIEWS);
    bounded.signatures.truncate(WIRE_PREVIEWS);
    omitted += bounded.bindings.len().saturating_sub(WIRE_PREVIEWS);
    bounded.bindings.truncate(WIRE_PREVIEWS);
    omitted += bounded.locations.len().saturating_sub(WIRE_PREVIEWS);
    bounded.locations.truncate(WIRE_PREVIEWS);
    omitted += bounded.requirements.len().saturating_sub(WIRE_PREVIEWS);
    bounded.requirements.truncate(WIRE_PREVIEWS);
    omitted += bounded.scenarios.len().saturating_sub(WIRE_PREVIEWS);
    bounded.scenarios.truncate(WIRE_PREVIEWS);
    omitted += bounded.gates.len().saturating_sub(WIRE_PREVIEWS);
    bounded.gates.truncate(WIRE_PREVIEWS);
    omitted += bounded.relations.len().saturating_sub(WIRE_RELATIONS);
    bounded.relations.truncate(WIRE_RELATIONS);
    omitted += bounded.effects.len().saturating_sub(WIRE_RELATIONS);
    bounded.effects.truncate(WIRE_RELATIONS);
    bounded.omitted = omitted.min(u32::MAX as usize) as u32;
    bounded
}

/// Serialize the exact compact response bytes (bounded): the stable
/// envelope status first, then the closed payload fields.
fn response_bytes(result: &SearchResult) -> Result<String, crate::diagnostics::DiagnosticSet> {
    let value = serde_json::to_value(result).map_err(|_| emit::result_limit("response", true))?;
    let payload = canonical_json(&value);
    let json = format!("{{\"status\":\"valid\",{}", &payload[1..]);
    if json.len() > MAX_RESPONSE_BYTES {
        return Err(emit::result_limit("response-bytes", false));
    }
    Ok(json)
}

/// The bounded human projection of one page.
fn human_summary(result: &SearchResult) -> String {
    let mut lines = vec![format!(
        "search {} : {} of {} (selection {})",
        if result.query.is_empty() {
            "(filters)"
        } else {
            result.query.as_str()
        },
        result.page.returned,
        result.page.visible_total.unwrap_or_default(),
        result.selection
    )];
    for item in &result.items {
        let card = &item.card;
        let module = card.module.as_deref().unwrap_or("-");
        lines.push(format!(
            "{} {} [{}] {} ({}, {})",
            card.key,
            card.kind.key(),
            module,
            card.description.as_deref().unwrap_or(""),
            item.r#match.tier,
            item.r#match.field
        ));
    }
    if result.page.truncated {
        lines.push("results truncated at the work cap".to_owned());
    }
    lines.join("\n")
}

/// The pure filter stage shared by `find` and `search`.
fn filter_cards<'a>(
    index: &'a SearchIndex,
    filters: &FilterSet,
) -> Result<Vec<&'a Card>, crate::diagnostics::DiagnosticSet> {
    let mut out = Vec::new();
    for card in &index.cards {
        if !admits(index, card, filters) {
            continue;
        }
        out.push(card);
    }
    out.sort_by(|left, right| left.key.as_bytes().cmp(right.key.as_bytes()));
    Ok(out)
}

/// Whether one card passes the whole AND-of-ORs exact filter set.
fn admits(index: &SearchIndex, card: &Card, filters: &FilterSet) -> bool {
    fn any_of(list: &[String], predicate: impl Fn(&str) -> bool) -> bool {
        list.is_empty() || list.iter().any(|value| predicate(value))
    }
    if !any_of(&filters.ids, |id| {
        card.id.as_deref() == Some(id) || card.key == format!("def:{id}")
    }) {
        return false;
    }
    if !any_of(&filters.kinds, |kind| {
        CardKind::parse(kind) == Some(card.kind)
    }) {
        return false;
    }
    if !any_of(&filters.modules, |module| {
        card.module.as_deref() == Some(module)
    }) {
        return false;
    }
    if !any_of(&filters.reads, |selector| {
        witness_matches(
            card,
            "reads",
            selector,
            filters.effect_scope,
            filters.include_stale,
        )
    }) {
        return false;
    }
    if !any_of(&filters.writes, |selector| {
        witness_matches(
            card,
            "writes",
            selector,
            filters.effect_scope,
            filters.include_stale,
        )
    }) {
        return false;
    }
    if !any_of(&filters.calls, |callee| {
        card.relations.iter().any(|witness| {
            witness.relation == "endpoint-invokes" && witness.other == format!("def:{callee}")
        })
    }) {
        return false;
    }
    if !any_of(&filters.requirements, |requirement| {
        (card.kind == CardKind::Requirement && card.id.as_deref() == Some(requirement))
            || card
                .requirements
                .iter()
                .any(|reference| reference_match(reference, requirement, filters.include_stale))
    }) {
        return false;
    }
    let matches_requirement_source = any_of(&filters.requirement_sources, |source| {
        let prefix = format!("req:{source}:");
        (card.kind == CardKind::Requirement && card.key.starts_with(&prefix))
            || card
                .requirements
                .iter()
                .any(|reference| reference_source_match(reference, &prefix, filters.include_stale))
    });
    if !matches_requirement_source {
        return false;
    }
    if !any_of(&filters.binding_paths, |path| {
        binding_path_matches(card, path)
    }) {
        return false;
    }
    if !any_of(&filters.native_symbols, |symbol| {
        card.bindings.iter().any(|binding| {
            // The exact native spelling, the stable binding key, and
            // the bare symbol half of a `path#symbol` spelling all
            // answer: validating path and symbol separately must not
            // lose either lookup.
            binding.symbol.as_deref() == Some(symbol)
                || binding.key == symbol
                || binding
                    .symbol
                    .as_deref()
                    .and_then(|spelling| spelling.rsplit_once('#'))
                    .is_some_and(|(_, bare)| bare == symbol)
        })
    }) {
        return false;
    }
    if !any_of(&filters.targets, |target| {
        card.bindings.iter().any(|binding| binding.target == target)
            || card
                .relations
                .iter()
                .any(|witness| witness.other == format!("target:{target}"))
    }) {
        return false;
    }
    if !any_of(&filters.types, |type_id| {
        card.signatures
            .iter()
            .any(|signature| signature.id == type_id)
    }) {
        return false;
    }
    if !any_of(&filters.input_types, |type_id| {
        card.signatures
            .iter()
            .any(|signature| signature.role == "input" && signature.id == type_id)
    }) {
        return false;
    }
    if !any_of(&filters.output_types, |type_id| {
        card.signatures
            .iter()
            .any(|signature| signature.role == "output" && signature.id == type_id)
    }) {
        return false;
    }
    if !any_of(&filters.errors, |error_id| {
        card.signatures
            .iter()
            .any(|signature| signature.role == "error" && signature.id == error_id)
    }) {
        return false;
    }
    if !filters.relations.is_empty() && !filters.related_to.is_empty() {
        let matched = filters.relations.iter().any(|relation| {
            filters.related_to.iter().any(|related| {
                card.relations.iter().any(|witness| {
                    witness_matches_relation(witness, relation, related, filters.include_stale)
                })
            })
        });
        if !matched {
            return false;
        }
    }
    if !any_of(&filters.ownerships, |ownership| {
        // Ownership is its own typed property (artifact ownership /
        // observed status), deliberately distinct from Model
        // visibility: `--ownership observed` resolves recorded observed
        // ownership, `module`/`project` resolve recorded ownership, and
        // neither ever reads (or overrides) the Model's visibility.
        card.ownership.as_deref() == Some(ownership)
    }) {
        return false;
    }
    if !any_of(&filters.portabilities, |portability| {
        card.portability.as_deref() == Some(portability)
    }) {
        return false;
    }
    if !any_of(&filters.profiles, |profile| {
        card.relations
            .iter()
            .any(|witness| witness.other == format!("profile:{profile}"))
    }) {
        return false;
    }
    if !any_of(&filters.diagnostics, |rule| {
        (card.kind == CardKind::Diagnostic && card.id.as_deref() == Some(rule))
            || card
                .relations
                .iter()
                .any(|witness| witness.other == format!("diag:{rule}"))
    }) {
        return false;
    }
    if !any_of(&filters.waivers, |waiver| {
        (card.kind == CardKind::Waiver && card.id.as_deref() == Some(waiver))
            || card
                .relations
                .iter()
                .any(|witness| witness.other == format!("waiv:{waiver}"))
    }) {
        return false;
    }
    // Stale rows are excluded unless explicitly included.
    if !filters.include_stale && card.freshness == crate::search::projection::Freshness::Stale {
        return false;
    }
    // Diagnostics/waivers/gates cards come from their producers only;
    // the index records their provenance and the filter stage here.
    let _ = index;
    true
}

/// Whether one joined requirement ref matches the selector. A ref the
/// trace materializer marked `stale:` (its relationship chain ran only
/// through stale relations) matches ONLY under the explicit
/// `--include-stale` path — relationship staleness is never laundered
/// into a current join by silence (issue #93 r5).
fn reference_match(reference: &str, requirement: &str, include_stale: bool) -> bool {
    match reference.strip_prefix("stale:") {
        Some(_) if !include_stale => false,
        Some(rest) => rest == requirement || rest.ends_with(&format!(":{requirement}")),
        None => reference == requirement || reference.ends_with(&format!(":{requirement}")),
    }
}

/// Whether one joined requirement ref carries the provider-namespace
/// qualifier the `--requirement-source` selector names (stale-marked
/// refs honor `--include-stale` exactly like `reference_match`).
fn reference_source_match(reference: &str, prefix: &str, include_stale: bool) -> bool {
    match reference.strip_prefix("stale:") {
        Some(_) if !include_stale => false,
        Some(rest) => rest.starts_with(prefix),
        None => reference.starts_with(prefix),
    }
}

/// Whether one effect witness matches a `resource` / `resource.field`
/// selector under the closed scope. Stale-trust witnesses are retained
/// per fact and match only under the explicit include-stale path — they
/// are never laundered into current evidence by silence either way.
fn witness_matches(
    card: &Card,
    action: &str,
    selector: &str,
    scope: EffectScope,
    include_stale: bool,
) -> bool {
    card.effects.iter().any(|witness| {
        if witness.trust == crate::search::projection::Trust::Stale && !include_stale {
            return false;
        }
        if !witness.relation.starts_with(&format!("{action}:")) {
            return false;
        }
        let subject = &witness.relation[action.len() + 1..];
        let (resource, field) = match subject.rsplit_once(':') {
            Some((resource, field)) => (resource, Some(field)),
            None => (subject, None),
        };
        match field {
            // Entity-wide witness: an exact entity selector names this
            // scope; a field selector admits it only as possible overlap.
            None => {
                if resource == selector {
                    true
                } else {
                    match selector.rsplit_once('.') {
                        Some((wanted_resource, _)) if wanted_resource == resource => {
                            scope == EffectScope::Overlapping
                        }
                        _ => false,
                    }
                }
            }
            // Field-scoped witness: a single field effect is never an
            // entity-wide write, so an exact entity selector refuses.
            Some(name) => {
                if resource == selector {
                    false
                } else {
                    match selector.rsplit_once('.') {
                        Some((wanted_resource, wanted_field)) => {
                            resource == wanted_resource && name == wanted_field
                        }
                        None => false,
                    }
                }
            }
        }
    })
}

/// Whether one direct incident relation witness matches. Stale-trust
/// witnesses (a trace relation recorded `stale`) match only under the
/// explicit include-stale path, exactly like stale effect witnesses.
fn witness_matches_relation(
    witness: &RelationWitness,
    relation: &str,
    related: &str,
    include_stale: bool,
) -> bool {
    if witness.trust == crate::search::projection::Trust::Stale && !include_stale {
        return false;
    }
    let wanted = if related.starts_with("def:")
        || related.starts_with("req:")
        || related.starts_with("target:")
    {
        related.to_owned()
    } else {
        format!("def:{related}")
    };
    witness.relation == relation
        && (witness.other == wanted || witness.other.ends_with(&format!(":{related}")))
}

/// Whether a `--binding-path` selector (exact relative path or a
/// directory prefix at component boundaries) matches a recorded native
/// binding. `planner` never matches `planner-old`.
fn binding_path_matches(card: &Card, selector: &str) -> bool {
    card.bindings.iter().any(|binding| {
        let path = binding.path.as_str();
        path == selector
            || path
                .strip_prefix(selector)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// The digest of the admitted policy facts (cursor-bound).
pub fn policy_digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_digest(bytes))
}

/// Whether a digest spelling is accepted for cursor bindings.
pub fn valid_binding_digest(digest: &str) -> bool {
    is_sha256(digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::projection::{
        Card, CardKind, Completeness, Confidence, Freshness, NativeBinding,
    };

    fn card(key: &str, id: &str, name: &str, description: &str) -> Card {
        Card {
            key: key.to_owned(),
            kind: CardKind::Command,
            subkind: Some("command".to_owned()),
            module: Some("planner".to_owned()),
            id: Some(id.to_owned()),
            name: Some(name.to_owned()),
            description: Some(description.to_owned()),
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
            ownership: None,
            confidence: Confidence::Canonical,
            freshness: Freshness::Current,
            completeness: Completeness::Complete,
            omitted: 0,
            provenance: "ir".to_owned(),
        }
    }

    fn index_of(cards: Vec<Card>) -> SearchIndex {
        let postings = std::collections::BTreeMap::new();
        let qualified = std::collections::BTreeMap::new();
        let mut projection = crate::search::projection::Projection {
            cards: cards
                .into_iter()
                .map(|card| (card.key.clone(), card))
                .collect(),
            postings,
            qualified_ids: qualified,
        };
        let cap = crate::search::version::MAX_POSTINGS;
        for (_, card) in projection.cards.clone() {
            crate::search::projection::add_card(&mut projection, card, cap);
        }
        SearchIndex::from_projection(
            &projection,
            &[],
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            Some("planner"),
            cap,
        )
        .expect("index")
    }

    #[test]
    fn exact_id_ranks_first_over_attractive_distractors() {
        let index = index_of(vec![
            card(
                "def:planner.focus_tasks",
                "planner.focus_tasks",
                "focus_tasks",
                "Focus many tasks at once",
            ),
            card(
                "def:planner.focus_task",
                "planner.focus_task",
                "focus_task",
                "Focus one task",
            ),
            card(
                "def:planner.focus",
                "planner.focus",
                "focus",
                "The focus operation",
            ),
        ]);
        let request = QueryRequest::admit(
            Some("planner.focus_task".to_owned()),
            true,
            Some(2),
            None,
            FilterSet::default(),
        )
        .expect("admits");
        let outcome = execute(
            &index,
            "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            Some("planner"),
            &request,
            vec![CoverageRow {
                domain: "ir".to_owned(),
                state: "complete".to_owned(),
            }],
        )
        .expect("executes");
        assert_eq!(outcome.result.items[0].card.key, "def:planner.focus_task");
        assert_eq!(outcome.result.items[0].r#match.tier, "exact-id");
    }

    #[test]
    fn ambiguity_is_reported_not_silently_resolved() {
        let index = index_of(vec![
            card("def:a.load", "a.load", "load", "Load it"),
            card("def:b.load", "b.load", "load", "Load it"),
        ]);
        let request = QueryRequest::admit(
            Some("load".to_owned()),
            false,
            None,
            None,
            FilterSet::default(),
        )
        .expect("admits");
        let outcome = execute(
            &index,
            "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            None,
            &request,
            vec![],
        )
        .expect("executes");
        assert_eq!(outcome.result.selection, "ambiguous");
        assert_eq!(outcome.result.items.len(), 2);
        assert!(outcome
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id() == "search.ambiguous"));
    }

    #[test]
    fn pagination_concatenates_without_repeats_or_skips() {
        let cards: Vec<Card> = (0..7)
            .map(|index| {
                card(
                    &format!("def:z.item{index}"),
                    &format!("z.item{index}"),
                    "item",
                    "Shared lexical item body",
                )
            })
            .collect();
        let index = index_of(cards);
        let policy = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
        let digest = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
        let mut seen: Vec<String> = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..10 {
            let request = QueryRequest::admit(
                Some("item".to_owned()),
                false,
                Some(3),
                cursor.clone(),
                FilterSet::default(),
            )
            .expect("admits");
            let outcome =
                execute(&index, digest, policy, None, &request, vec![]).expect("executes");
            for item in &outcome.result.items {
                seen.push(item.card.key.clone());
            }
            match &outcome.result.page.next {
                Some(next) => cursor = Some(next.clone()),
                None => break,
            }
        }
        assert_eq!(seen.len(), 7);
        let mut sorted = seen.clone();
        sorted.sort();
        seen.sort();
        assert_eq!(seen, sorted);
    }

    #[test]
    fn cursor_rejects_a_different_generation() {
        let index = index_of(vec![card("def:a.one", "a.one", "one", "one")]);
        let request = QueryRequest::admit(
            Some("one".to_owned()),
            false,
            Some(1),
            Some("AAAA:BBBB".to_owned()),
            FilterSet::default(),
        )
        .expect("admits");
        let error = execute(
            &index,
            "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            None,
            &request,
            vec![],
        )
        .expect_err("refused");
        assert!(error
            .reason_ids()
            .contains(&"search.cursor-invalid".to_owned().as_str()));
    }

    #[test]
    fn binding_path_never_crosses_component_boundaries() {
        let binding = NativeBinding {
            key: "k".to_owned(),
            target: "node-typescript".to_owned(),
            path: "apps/api/src/planner-old/legacy.ts".to_owned(),
            symbol: None,
            status: "inferred".to_owned(),
        };
        let mut card = card("def:a.x", "a.x", "x", "x");
        card.bindings.push(binding);
        assert!(binding_path_matches(&card, "apps/api/src/planner-old"));
        assert!(!binding_path_matches(&card, "apps/api/src/planner"));
    }

    #[test]
    fn exact_id_survives_an_exhausted_fuzzy_budget() {
        // 10,005 preceding commands plus one exact-ID target: the fuzzy
        // work budget must never suppress the exact match (AC2), and an
        // exhausted budget must not assert exact totals or uniqueness.
        let mut cards: Vec<Card> = (0..10_005)
            .map(|index| {
                card(
                    &format!("def:planner.filler{index}"),
                    &format!("planner.filler{index}"),
                    "filler",
                    "filler command",
                )
            })
            .collect();
        cards.push(card(
            "def:planner.zzexact",
            "planner.zzexact",
            "zzexact",
            "exact command",
        ));
        let index = index_of(cards);
        let request = QueryRequest::admit(
            Some("planner.zzexact".to_owned()),
            true,
            Some(10),
            None,
            FilterSet::default(),
        )
        .expect("admits");
        let outcome = execute(
            &index,
            "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            Some("planner"),
            &request,
            vec![CoverageRow {
                domain: "ir".to_owned(),
                state: "complete".to_owned(),
            }],
        )
        .expect("executes");
        assert!(
            outcome
                .result
                .items
                .iter()
                .any(|item| item.card.key == "def:planner.zzexact"),
            "the exact ID must be present even with fuzzy enabled"
        );
        assert_eq!(outcome.result.items[0].card.key, "def:planner.zzexact");
        assert_eq!(outcome.result.items[0].r#match.tier, "exact-id");
    }

    #[test]
    fn exhausted_fuzzy_budget_never_asserts_exact_totals_or_uniqueness() {
        // Beyond the fuzzy candidate cap with NO exact match: the page
        // is honestly truncated, the total is withheld, and a lone
        // fuzzy survivor is never a confirmed unique answer.
        let cards: Vec<Card> = (0..10_050)
            .map(|index| {
                card(
                    &format!("def:m.fokus{index}"),
                    &format!("m.fokus{index}"),
                    &format!("fokus{index}"),
                    "fokus command",
                )
            })
            .collect();
        let index = index_of(cards);
        let request = QueryRequest::admit(
            Some("fokuz".to_owned()),
            true,
            Some(5),
            None,
            FilterSet::default(),
        )
        .expect("admits");
        let outcome = execute(
            &index,
            "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            None,
            &request,
            vec![CoverageRow {
                domain: "ir".to_owned(),
                state: "complete".to_owned(),
            }],
        )
        .expect("executes");
        assert!(
            outcome.result.page.truncated,
            "the exhausted budget is disclosed"
        );
        assert_eq!(
            outcome.result.page.visible_total, None,
            "no exact total over unfinished work"
        );
        assert_ne!(
            outcome.result.selection, "unique",
            "unfinished work never confirms unique"
        );
        assert!(outcome
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.id() == "search.result-limit"));
    }

    #[test]
    fn ownership_filters_the_typed_property_not_visibility() {
        let mut observed = card("def:a.entity", "a.entity", "entity", "entity");
        observed.ownership = Some("observed".to_owned());
        observed.visibility = Some("module".to_owned());
        let mut declared = card("def:b.entity", "b.entity", "entity", "entity");
        declared.ownership = Some("project".to_owned());
        declared.visibility = Some("project".to_owned());
        let index = index_of(vec![observed, declared]);
        let run = |ownerships: &[&str]| {
            let filters = FilterSet {
                ownerships: ownerships.iter().map(|value| value.to_string()).collect(),
                ..FilterSet::default()
            };
            let request = QueryRequest::admit(None, false, None, None, filters).expect("admits");
            execute(
                &index,
                "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                None,
                &request,
                vec![],
            )
            .expect("executes")
        };
        // `--ownership observed` resolves the typed observed property.
        let outcome = run(&["observed"]);
        assert_eq!(outcome.result.items.len(), 1);
        assert_eq!(outcome.result.items[0].card.key, "def:a.entity");
        // `--ownership module` resolves RECORDED ownership only: the
        // observed card's Model visibility is `module`, but that
        // property never answers an ownership question.
        assert_eq!(run(&["module"]).result.selection, "none");
        let outcome = run(&["project"]);
        assert_eq!(outcome.result.items.len(), 1);
        assert_eq!(outcome.result.items[0].card.key, "def:b.entity");
        // Both properties ride the card independently.
        assert_eq!(
            outcome.result.items[0].card.ownership.as_deref(),
            Some("project")
        );
    }

    #[test]
    fn stale_effect_witnesses_are_retained_but_match_only_when_included() {
        let mut card = card("def:a.store", "a.store", "store", "store");
        card.effects
            .push(crate::search::projection::RelationWitness {
                relation: "writes:planner.row".to_owned(),
                other: "resource:planner.row".to_owned(),
                direction: "outgoing".to_owned(),
                trust: crate::search::projection::Trust::Stale,
                confidence: Confidence::Unknown,
            });
        let index = index_of(vec![card]);
        let run = |include_stale: bool| {
            let filters = FilterSet {
                writes: vec!["planner.row".to_owned()],
                include_stale,
                ..FilterSet::default()
            };
            let request = QueryRequest::admit(None, false, None, None, filters).expect("admits");
            execute(
                &index,
                "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                None,
                &request,
                vec![],
            )
            .expect("executes")
        };
        assert_eq!(
            run(false).result.selection,
            "none",
            "stale evidence never matches by default"
        );
        let included = run(true);
        assert_eq!(included.result.selection, "unique");
        assert!(included.result.items[0]
            .card
            .effects
            .iter()
            .any(|witness| witness.trust.key() == "stale"));
    }
}
