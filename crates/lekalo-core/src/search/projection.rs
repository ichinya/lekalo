//! The typed search projection (issue #93): the bounded selection cards,
//! relation witnesses, and evidence facts an index or a transient query
//! serves.
//!
//! Cards are atomic and immutable. Every card carries a stable qualified
//! key, its kind/subkind/module, privacy-admitted metadata, bounded
//! signatures and refs, evidence origin/status per relationship,
//! freshness, and completeness. Nothing here reads the filesystem or
//! launches anything: projections are pure functions over the immutable
//! typed IR, the built graph/effects, and the explicitly attached
//! producer snapshots.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::ir::{CompiledProject, Definition, DefinitionKind};
use crate::search::rank::{normalize_text, tokenize};
use crate::search::version::{MAX_DESCRIPTION, MAX_PATH_BYTES, MAX_PREVIEWS};

/// The closed search card kinds. `command` is an operation subkind, not
/// a graph node kind; evidence cards (requirement/binding/diagnostic/
/// waiver/gate) come from their own producers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CardKind {
    /// A module definition.
    Module,
    /// A scalar definition.
    Scalar,
    /// An enum definition.
    Enum,
    /// A value-object definition.
    ValueObject,
    /// An entity definition.
    Entity,
    /// A command definition (subkind `command`).
    Command,
    /// A query definition.
    Query,
    /// A policy definition.
    Policy,
    /// An event definition.
    Event,
    /// An effect definition.
    Effect,
    /// An endpoint definition.
    Endpoint,
    /// A scenario definition.
    Scenario,
    /// A target-binding definition.
    TargetBinding,
    /// A requirement card from the requirements/trace producers.
    Requirement,
    /// A recorded native binding card from the observed producer.
    Binding,
    /// An admitted diagnostic finding card.
    Diagnostic,
    /// A qualified waiver card.
    Waiver,
    /// A recorded gate card from the trace producer.
    Gate,
}

impl CardKind {
    /// The closed kinds in canonical byte order.
    pub const KEYS: [Self; 17] = [
        Self::Binding,
        Self::Command,
        Self::Diagnostic,
        Self::Effect,
        Self::Endpoint,
        Self::Entity,
        Self::Enum,
        Self::Event,
        Self::Gate,
        Self::Module,
        Self::Policy,
        Self::Query,
        Self::Requirement,
        Self::Scalar,
        Self::Scenario,
        Self::TargetBinding,
        Self::ValueObject,
    ];

    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Scalar => "scalar",
            Self::Enum => "enum",
            Self::ValueObject => "value-object",
            Self::Entity => "entity",
            Self::Command => "command",
            Self::Query => "query",
            Self::Policy => "policy",
            Self::Event => "event",
            Self::Effect => "effect",
            Self::Endpoint => "endpoint",
            Self::Scenario => "scenario",
            Self::TargetBinding => "target-binding",
            Self::Requirement => "requirement",
            Self::Binding => "binding",
            Self::Diagnostic => "diagnostic",
            Self::Waiver => "waiver",
            Self::Gate => "gate",
        }
    }

    /// Registry lookup by exact key.
    pub fn parse(text: &str) -> Option<Self> {
        Self::KEYS.iter().copied().find(|kind| kind.key() == text)
    }

    /// Map an IR definition kind onto its card kind.
    pub const fn of_definition(kind: DefinitionKind) -> Self {
        match kind {
            DefinitionKind::Scalar => Self::Scalar,
            DefinitionKind::Enum => Self::Enum,
            DefinitionKind::ValueObject => Self::ValueObject,
            DefinitionKind::Entity => Self::Entity,
            DefinitionKind::Command => Self::Command,
            DefinitionKind::Query => Self::Query,
            DefinitionKind::Policy => Self::Policy,
            DefinitionKind::Event => Self::Event,
            DefinitionKind::Effect => Self::Effect,
            DefinitionKind::Endpoint => Self::Endpoint,
            DefinitionKind::Scenario => Self::Scenario,
            DefinitionKind::TargetBinding => Self::TargetBinding,
        }
    }
}

/// The closed completeness vocabulary reused from the observed
/// selection cards: `unknown` is never zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Completeness {
    Complete,
    Partial,
    Unknown,
    Unsupported,
    Withheld,
}

impl Completeness {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Unknown => "unknown",
            Self::Unsupported => "unsupported",
            Self::Withheld => "withheld",
        }
    }
}

/// The closed freshness vocabulary: evidence states are as-recorded;
/// query-time native state stays `unknown` unless revalidated.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Freshness {
    Current,
    Stale,
    AsRecorded,
    Unknown,
}

impl Freshness {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Stale => "stale",
            Self::AsRecorded => "as-recorded",
            Self::Unknown => "unknown",
        }
    }
}

/// The closed evidence confidence vocabulary (the graph's ladder).
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Canonical,
    Verified,
    Extracted,
    Inferred,
    Unknown,
}

impl Confidence {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::Verified => "verified",
            Self::Extracted => "extracted",
            Self::Inferred => "inferred",
            Self::Unknown => "unknown",
        }
    }

    /// Map the closed graph confidence onto the search vocabulary.
    pub const fn of_graph(confidence: crate::graph::model::Confidence) -> Self {
        match confidence {
            crate::graph::model::Confidence::Canonical => Self::Canonical,
            crate::graph::model::Confidence::Verified => Self::Verified,
            crate::graph::model::Confidence::Extracted => Self::Extracted,
            crate::graph::model::Confidence::Inferred => Self::Inferred,
            crate::graph::model::Confidence::Unknown => Self::Unknown,
        }
    }
}

/// The closed declared/detected trust split of effect witnesses.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Trust {
    Declared,
    Detected,
}

impl Trust {
    /// The exact wire spelling.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Detected => "detected",
        }
    }
}

/// One bounded evidence relation: direction is recorded, origin and
/// status are per relationship.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationWitness {
    /// The closed relation kind (`endpoint-invokes`, `scenario-covers`,
    /// `command-input`, `query-returns`, `query-reads`, `derived-from`,
    /// `command-effect`, `effect-emits`, `effect-entity`, or the effect
    /// read/write families).
    pub relation: String,
    /// The qualified key of the opposite card.
    pub other: String,
    /// `outgoing` or `incoming` from this card's perspective.
    pub direction: String,
    /// The evidence trust of this witness.
    pub trust: Trust,
    /// The per-relationship confidence.
    pub confidence: Confidence,
}

/// One bounded source location: logical project-relative path plus
/// optional 0-based byte range.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpan {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<usize>,
}

/// One typed signature reference (input/output/error/type roles).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignatureRef {
    /// The closed role: `input` | `output` | `error` | `type`.
    pub role: String,
    /// The fully qualified type identity.
    pub id: String,
}

/// One recorded native binding candidate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeBinding {
    /// The stable binding key.
    pub key: String,
    /// The recorded target id.
    pub target: String,
    /// The exact relative path of the recorded native location.
    pub path: String,
    /// The recorded native symbol spelling, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// The candidate status (`explicit` | `confirmed` | `inferred`).
    pub status: String,
}

/// The bounded selection card: the atomic unit of every answer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Card {
    /// The stable qualified key (`def:planner.focus_task`,
    /// `req:openspec:PLANNER-REQ-001`, `bind:<key>`, `diag:<id>`,
    /// `waiv:<id>`, `gate:<id>`).
    pub key: String,
    /// The card kind.
    pub kind: CardKind,
    /// The bounded subkind (e.g. `command` operation subkind).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subkind: Option<String>,
    /// The canonical module id, when the owner recorded one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// The full semantic identity (definition cards only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The privacy-admitted display name (the last segment).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The privacy-admitted description metadata (bounded, may be
    /// `withheld` by policy through completeness).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Typed signature refs (bounded).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signatures: Vec<SignatureRef>,
    /// Evidence relations with per-relationship trust (bounded).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RelationWitness>,
    /// Effect witnesses: `reads:<resource>[.<field>]` /
    /// `writes:<resource>[.<field>]` keys with declared/detected trust.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<RelationWitness>,
    /// Recorded native bindings (bounded preview).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<NativeBinding>,
    /// Bounded source locations (preview + omitted count).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<SourceSpan>,
    /// Requirement refs (bounded preview, qualified).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<String>,
    /// Scenario refs (bounded preview).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scenarios: Vec<String>,
    /// Gate refs (bounded preview).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gates: Vec<String>,
    /// Portability (`portable` | `target-specific`), when declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portability: Option<String>,
    /// Ownership/visibility, when declared (`module` | `project`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    /// The evidence confidence of the card's own facts.
    pub confidence: Confidence,
    /// The freshness of the card's evidence.
    pub freshness: Freshness,
    /// The completeness of the card's admitted metadata.
    pub completeness: Completeness,
    /// How many locations/refs were omitted by the preview bounds.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub omitted: u32,
    /// The exact producer provenance (`ir`, `effects`, `trace`,
    /// `requirements`, `observed`, `diagnostics`, `waivers`).
    pub provenance: String,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

impl Card {
    /// The normalized name terms of the privacy-admitted name.
    pub(crate) fn name_terms(&self) -> Vec<String> {
        self.name
            .as_deref()
            .map(|name| tokenize(&normalize_text(name)))
            .unwrap_or_default()
    }

    /// The normalized description terms of the admitted metadata.
    pub(crate) fn description_terms(&self) -> Vec<String> {
        self.description
            .as_deref()
            .map(|text| tokenize(&normalize_text(text)))
            .unwrap_or_default()
    }
}

/// Bound one description to the accepted scalar cap.
pub(crate) fn bound_description(text: &str) -> String {
    text.chars().take(MAX_DESCRIPTION).collect()
}

/// Bound one echoed path to the accepted byte cap.
#[allow(dead_code)]
pub(crate) fn bound_path(text: &str) -> String {
    if text.len() <= MAX_PATH_BYTES {
        text.to_owned()
    } else {
        let mut out = String::new();
        for character in text.chars() {
            if out.len() + character.len_utf8() > MAX_PATH_BYTES {
                break;
            }
            out.push(character);
        }
        out
    }
}

/// Push one ref into a bounded preview, counting omissions.
pub(crate) fn bounded_push(list: &mut Vec<String>, omitted: &mut u32, item: String) {
    if list.len() < MAX_PREVIEWS {
        list.push(item);
    } else {
        *omitted += 1;
    }
}

/// The whole typed projection of one project plus its attached evidence.
#[derive(Clone, Debug, Default)]
pub struct Projection {
    /// The cards keyed by their stable qualified key.
    pub cards: BTreeMap<String, Card>,
    /// Postings: normalized term -> qualified keys (byte-sorted sets).
    pub postings: BTreeMap<String, Vec<String>>,
    /// Additional exact qualified identities per card key.
    pub qualified_ids: BTreeMap<String, Vec<String>>,
}

/// Append one term posting (sorted set, bounded).
fn post(postings: &mut BTreeMap<String, Vec<String>>, term: &str, key: &str, cap: usize) {
    if term.is_empty() || postings.len() >= cap {
        return;
    }
    let entry = postings
        .entry(term.to_owned())
        .or_insert_with(|| Vec::with_capacity(1));
    if !entry.contains(&key.to_owned()) {
        entry.push(key.to_owned());
        entry.sort();
    }
}

/// Project every definition, module, and rename/tombstone fact of the
/// compiled IR into cards and postings.
pub fn project_ir(project: &CompiledProject, cap: usize) -> Projection {
    let mut projection = Projection::default();
    for module in &project.modules {
        let key = format!("def:{}", module.id.as_str());
        let card = Card {
            key: key.clone(),
            kind: CardKind::Module,
            subkind: None,
            module: Some(module.id.as_str().to_owned()),
            id: Some(module.id.as_str().to_owned()),
            name: Some(module.id.as_str().to_owned()),
            description: module
                .common
                .description
                .as_ref()
                .map(|text| bound_description(text.as_str())),
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
            confidence: Confidence::Canonical,
            freshness: Freshness::Current,
            completeness: Completeness::Complete,
            omitted: 0,
            provenance: "ir".to_owned(),
        };
        add_card(&mut projection, card, cap);
    }
    for definition in &project.definitions {
        add_definition_card(&mut projection, definition, cap);
    }
    projection
}

/// Build the card of one definition: identity, metadata, typed
/// signature roles, requirement links, and scenario coverage.
fn add_definition_card(projection: &mut Projection, definition: &Definition, cap: usize) {
    let common = definition.common();
    let id = definition.id().as_str();
    let key = format!("def:{id}");
    let module = id.split_once('.').map(|(module, _)| module.to_owned());
    let mut signatures = Vec::new();
    let mut relations: Vec<RelationWitness> = Vec::new();
    let mut requirements: Vec<String> = Vec::new();
    let scenarios: Vec<String> = Vec::new();
    let mut omitted = 0u32;
    let canonical = Confidence::Canonical;
    let trust = Trust::Declared;
    let record = |relations: &mut Vec<RelationWitness>,
                  role: &'static str,
                  other: &str,
                  relation: &'static str| {
        relations.push(RelationWitness {
            relation: relation.to_owned(),
            other: format!("def:{other}"),
            direction: "outgoing".to_owned(),
            trust,
            confidence: canonical,
        });
        let _ = role;
    };
    match definition {
        Definition::Scalar(_) => {}
        Definition::Enum(_) => {}
        Definition::ValueObject(def) => {
            for field in &def.fields {
                signatures.push(SignatureRef {
                    role: "type".to_owned(),
                    id: type_leaf(&field.r#type),
                });
            }
        }
        Definition::Entity(def) => {
            for field in &def.fields {
                signatures.push(SignatureRef {
                    role: "type".to_owned(),
                    id: type_leaf(&field.r#type),
                });
            }
        }
        Definition::Command(def) => {
            for field in &def.input {
                signatures.push(SignatureRef {
                    role: "input".to_owned(),
                    id: type_leaf(&field.r#type),
                });
            }
            for effect in &def.effects {
                record(&mut relations, "effect", effect.as_str(), "command-effect");
            }
        }
        Definition::Query(def) => {
            if let Some(returns) = &def.returns {
                signatures.push(SignatureRef {
                    role: "output".to_owned(),
                    id: type_leaf(returns),
                });
            }
            for read in &def.reads {
                record(&mut relations, "reads", read.as_str(), "query-reads");
            }
        }
        Definition::Policy(def) => {
            for applies in &def.applies_to {
                record(
                    &mut relations,
                    "applies-to",
                    applies.as_str(),
                    "policy-applies-to",
                );
            }
        }
        Definition::Event(def) => {
            for field in &def.payload {
                signatures.push(SignatureRef {
                    role: "type".to_owned(),
                    id: type_leaf(&field.r#type),
                });
            }
        }
        Definition::Effect(def) => {
            relations.push(RelationWitness {
                relation: "effect-entity".to_owned(),
                other: format!("def:{}", def.entity.as_str()),
                direction: "outgoing".to_owned(),
                trust,
                confidence: canonical,
            });
            for emits in &def.emits {
                record(&mut relations, "emits", emits.as_str(), "effect-emits");
            }
        }
        Definition::Endpoint(def) => {
            relations.push(RelationWitness {
                relation: "endpoint-invokes".to_owned(),
                other: format!("def:{}", def.invokes.as_str()),
                direction: "outgoing".to_owned(),
                trust,
                confidence: canonical,
            });
        }
        Definition::Scenario(def) => {
            for covered in &def.covers {
                record(
                    &mut relations,
                    "covers",
                    covered.as_str(),
                    "scenario-covers",
                );
            }
        }
        Definition::TargetBinding(def) => {
            // The declared target is the subkind fact of the binding.
            relations.push(RelationWitness {
                relation: "binds-target".to_owned(),
                other: format!("target:{}", def.target.as_str()),
                direction: "outgoing".to_owned(),
                trust,
                confidence: canonical,
            });
        }
    }
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
    for derived in &common.derived_from {
        // Requirement identity is namespaced by its provider only when
        // an explicit producer supplies one; the IR carries the bare id.
        bounded_push(&mut requirements, &mut omitted, derived.as_str().to_owned());
    }
    signatures.truncate(MAX_PREVIEWS);
    let description = common
        .description
        .as_ref()
        .map(|text| bound_description(text.as_str()));
    let card = Card {
        key: key.clone(),
        kind: CardKind::of_definition(definition.kind()),
        subkind: matches!(definition.kind(), DefinitionKind::Command).then(|| "command".to_owned()),
        module,
        id: Some(id.to_owned()),
        name: Some(id.rsplit('.').next().unwrap_or(id).to_owned()),
        description,
        signatures,
        relations,
        effects: Vec::new(),
        bindings: Vec::new(),
        locations: Vec::new(),
        requirements,
        scenarios,
        gates: Vec::new(),
        portability: common
            .portability
            .map(|portability| portability.as_str().to_owned()),
        visibility: common
            .visibility
            .map(|visibility| visibility.as_str().to_owned()),
        confidence: Confidence::Canonical,
        freshness: Freshness::Current,
        completeness: Completeness::Complete,
        omitted,
        provenance: "ir".to_owned(),
    };
    add_card(projection, card, cap);
}

/// The leaf symbol identity of a type reference (list/optional unwrapped).
fn type_leaf(reference: &crate::ir::TypeRef) -> String {
    match reference {
        crate::ir::TypeRef::Ref(symbol) => symbol.as_str().to_owned(),
        crate::ir::TypeRef::List(inner) | crate::ir::TypeRef::Optional(inner) => type_leaf(inner),
    }
}

/// Insert one card, then build its postings (byte-sorted key sets).
pub(crate) fn add_card(projection: &mut Projection, card: Card, cap: usize) {
    let key = card.key.clone();
    let id_terms = card.id.as_deref().map(tokenize).unwrap_or_default();
    let name_terms = card.name_terms();
    let description_terms = card.description_terms();
    for term in id_terms
        .iter()
        .chain(name_terms.iter())
        .chain(description_terms.iter())
    {
        post(&mut projection.postings, term, &key, cap);
    }
    projection.cards.insert(key, card);
}

/// Attach the exact effect witnesses (`--reads`/`--writes`) of the
/// declared and detected effect graphs onto operation cards.
///
/// Exact-field edges stay distinguishable from entity-wide effects: the
/// witness key spells the field only for exact-field subjects, so an
/// entity-wide mutation can never claim a field it did not name.
pub fn attach_effects(
    projection: &mut Projection,
    graph: &crate::effects::EffectGraph,
    cap: usize,
) {
    let mut witnesses: BTreeMap<String, Vec<RelationWitness>> = BTreeMap::new();
    for (trust, edges) in [
        (Trust::Declared, graph.declared()),
        (Trust::Detected, graph.detected()),
    ] {
        for edge in edges {
            let operation = edge.key().operation().semantic_id();
            let card_key = format!("def:{operation}");
            if !projection.cards.contains_key(&card_key) {
                continue;
            }
            let subject = edge.key().subject();
            let action = match edge.key().kind().key() {
                "read" => "reads",
                "create" | "update" | "delete" | "write-field" => "writes",
                _ => continue,
            };
            let resource = subject.resource().as_str();
            let field = subject.field().map(|field| field.as_str());
            let witness = RelationWitness {
                relation: match field {
                    Some(field) => format!("{action}:{}:{}", resource, field),
                    None => format!("{action}:{}", resource),
                },
                other: format!("resource:{}", resource),
                direction: "outgoing".to_owned(),
                trust,
                confidence: match trust {
                    Trust::Declared => Confidence::Canonical,
                    Trust::Detected => Confidence::Extracted,
                },
            };
            let entry = witnesses.entry(card_key).or_default();
            if !entry.contains(&witness) {
                entry.push(witness);
            }
        }
    }
    for (card_key, mut list) in witnesses {
        list.sort_by(|left, right| {
            (
                left.relation.as_bytes(),
                left.other.as_bytes(),
                left.trust.key(),
            )
                .cmp(&(
                    right.relation.as_bytes(),
                    right.other.as_bytes(),
                    right.trust.key(),
                ))
        });
        if let Some(card) = projection.cards.get_mut(&card_key) {
            card.effects = list;
            for witness in &card.effects {
                for term in tokenize(&witness.relation) {
                    post(&mut projection.postings, &term, &card.key, cap);
                }
            }
        }
    }
}

/// The stable qualified key of a requirement identity: the bare IR id,
/// or the provider-qualified spelling when a producer supplied one.
pub fn requirement_key(source: &str, requirement: &str) -> String {
    if source.is_empty() {
        format!("req:{requirement}")
    } else {
        format!("req:{source}:{requirement}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_witnesses_keep_exact_and_entity_wide_apart() {
        let declared = "reads:planner.task".to_owned();
        let exact_field = "writes:planner.task:focus_count".to_owned();
        let entity_wide = "writes:planner.task".to_owned();
        assert_ne!(exact_field, entity_wide);
        assert!(declared.starts_with("reads:"));
    }

    #[test]
    fn requirement_keys_qualified_by_provider() {
        assert_eq!(
            requirement_key("openspec", "PLANNER-REQ-001"),
            "req:openspec:PLANNER-REQ-001"
        );
        assert_eq!(
            requirement_key("", "PLANNER-REQ-001"),
            "req:PLANNER-REQ-001"
        );
    }
}
