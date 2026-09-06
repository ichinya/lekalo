//! The normalized context capsule and its typed facts (issue #17).
//!
//! One capsule is the single normalized product both projections render:
//! the canonical JSON wire and the Markdown document. Facts are typed
//! records over the accepted #12/#13/#14 surfaces — protected semantic
//! facts are never collapsed into ambiguous prose. Every fact carries its
//! stable id, its section, its estimated token count, and its rank, so the
//! included/excluded manifest is explainable row by row.

use super::estimate;
use crate::diagnostics::types::bound_token;

/// The closed capsule section vocabulary in selection-rank order. The rank
/// is the deterministic inclusion order: protected semantic facts first,
/// ranked supporting context last.
pub(crate) const SECTION_KEYS: [&str; 9] = [
    "symbol",
    "policies",
    "effects",
    "dependencies",
    "scenarios",
    "public-impact",
    "bindings",
    "types",
    "closure",
];

/// The section rank of `key` in the closed vocabulary.
pub(crate) fn section_rank(key: &str) -> usize {
    SECTION_KEYS
        .iter()
        .position(|candidate| *candidate == key)
        .expect("section keys are closed")
}

/// Why one candidate fact is excluded from the emitted capsule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExclusionReason {
    /// The ranked budget walk could not fit the fact.
    Budget,
}

impl ExclusionReason {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Budget => "budget",
        }
    }
}

/// One typed fact of the capsule.
///
/// The variant carries every machine field; `json()` and `markdown()`
/// project the same record, and `content` feeds the fixed estimator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Fact {
    /// A root (or supporting) definition card: identity, purpose, and the
    /// canonical kind contract.
    Card(CardFact),
    /// A policy applying to a root.
    Policy {
        id: String,
        decision: &'static str,
        applies_to: Vec<String>,
        description: Option<String>,
    },
    /// One effect edge of a root operation.
    Effect(EffectFact),
    /// One graph edge (dependency, closure, or public-impact section).
    Edge(EdgeFact),
    /// A scenario directly covering a root.
    Scenario {
        id: String,
        summary: String,
        covers: Vec<String>,
    },
    /// A target binding of a root's module.
    Binding { id: String, target: String },
}

/// A definition card: identity plus the canonical kind contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CardFact {
    pub qualified: String,
    pub kind: String,
    pub subkind: Option<String>,
    pub module: Option<String>,
    pub version: Option<u64>,
    pub visibility: Option<&'static str>,
    pub portability: Option<&'static str>,
    pub derived_from: Vec<String>,
    pub description: Option<String>,
    pub contract: Option<Contract>,
    /// The type symbol this card was emitted for (supporting `types` cards
    /// only; empty for roots).
    pub covers_section: &'static str,
}

/// The canonical contract of one definition kind: exactly what the accepted
/// Model declares, nothing invented.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Contract {
    Scalar {
        base: &'static str,
    },
    Enum {
        values: Vec<(String, Option<String>)>,
    },
    Fields {
        fields: Vec<WireField>,
        identity: Vec<String>,
    },
    Command {
        input: Vec<WireField>,
        effects: Vec<String>,
    },
    Query {
        reads: Vec<String>,
        returns: Option<WireType>,
    },
    Policy {
        applies_to: Vec<String>,
        decision: &'static str,
    },
    Event {
        payload: Vec<WireField>,
    },
    Effect {
        operation: &'static str,
        entity: String,
        emits: Vec<String>,
    },
    Endpoint {
        invokes: String,
        method: &'static str,
        path: String,
    },
    Scenario {
        covers: Vec<String>,
        summary: String,
    },
    Binding {
        target: String,
    },
}

/// One typed field row of a contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WireField {
    pub name: String,
    pub required: bool,
    pub r#type: WireType,
}

/// The structured wire form of a type reference (never a string grammar).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WireType {
    Ref(String),
    List(Box<WireType>),
    Optional(Box<WireType>),
}

impl WireType {
    /// The loader type-sugar spelling (`ref`, `list<T>`, `T?`) for the
    /// Markdown projection and the estimator content.
    pub fn spell(&self) -> String {
        match self {
            Self::Ref(id) => id.clone(),
            Self::List(inner) => format!("list<{}>", inner.spell()),
            Self::Optional(inner) => format!("{}?", inner.spell()),
        }
    }
}

/// One effect edge fact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EffectFact {
    pub operation: String,
    pub kind: String,
    pub action: Option<&'static str>,
    pub resource_kind: String,
    pub resource: String,
    pub field: Option<String>,
    pub occurrence: u32,
    pub confidence: &'static str,
}

/// One graph edge fact (dependency, closure, or public-impact section).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EdgeFact {
    pub relation: String,
    pub from: String,
    pub to: String,
    pub occurrence: u32,
    pub confidence: &'static str,
}

/// One closed confidence-gap row.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct Gap {
    pub gap: &'static str,
    pub symbols: Vec<String>,
}

/// One manifest row: the explainable inclusion/exclusion record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManifestRow {
    pub id: String,
    pub section: &'static str,
    pub tokens: u64,
    pub reason: Option<ExclusionReason>,
}

impl Fact {
    /// The stable fact id (unique inside its section).
    pub(crate) fn id(&self) -> String {
        match self {
            Self::Card(card) => card.qualified.clone(),
            Self::Policy { id, .. } => id.clone(),
            Self::Effect(effect) => {
                let subject = match &effect.field {
                    Some(field) => format!("{}.{}", effect.resource, field),
                    None => effect.resource.clone(),
                };
                format!(
                    "effect:{}:{}->{}:{}#{}",
                    effect.kind, effect.operation, effect.resource_kind, subject, effect.occurrence
                )
            }
            Self::Edge(edge) => format!(
                "edge:{}:{}->{}#{}",
                edge.relation, edge.from, edge.to, edge.occurrence
            ),
            Self::Scenario { id, .. } => id.clone(),
            Self::Binding { id, .. } => id.clone(),
        }
    }

    /// The estimator content string: the semantic text values of the fact
    /// joined by single spaces, in a fixed order.
    pub(crate) fn content(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        match self {
            Self::Card(card) => {
                parts.push(card.kind.clone());
                parts.push(card.qualified.clone());
                parts.extend(card.subkind.clone());
                parts.extend(card.module.clone());
                parts.extend(card.version.map(|v| v.to_string()));
                parts.extend(card.visibility.map(str::to_owned));
                parts.extend(card.portability.map(str::to_owned));
                parts.extend(card.derived_from.iter().cloned());
                parts.extend(card.description.clone());
                contract_content(&card.contract, &mut parts);
            }
            Self::Policy {
                id,
                decision,
                applies_to,
                description,
            } => {
                parts.push(id.clone());
                parts.push((*decision).to_owned());
                parts.extend(applies_to.iter().cloned());
                parts.extend(description.clone());
            }
            Self::Effect(effect) => {
                parts.push(effect.operation.clone());
                parts.push(effect.kind.clone());
                parts.extend(effect.action.map(str::to_owned));
                parts.push(effect.resource_kind.clone());
                parts.push(effect.resource.clone());
                parts.extend(effect.field.clone());
                parts.push(effect.occurrence.to_string());
                parts.push(effect.confidence.to_owned());
            }
            Self::Edge(edge) => {
                parts.push(edge.relation.clone());
                parts.push(edge.from.clone());
                parts.push(edge.to.clone());
                parts.push(edge.occurrence.to_string());
                parts.push(edge.confidence.to_owned());
            }
            Self::Scenario {
                id,
                summary,
                covers,
            } => {
                parts.push(id.clone());
                parts.push(summary.clone());
                parts.extend(covers.iter().cloned());
            }
            Self::Binding { id, target } => {
                parts.push(id.clone());
                parts.push(target.clone());
            }
        }
        parts.join(" ")
    }

    /// The estimated token count of this fact.
    pub(crate) fn tokens(&self) -> u64 {
        estimate::tokens(&self.content())
    }

    /// The exact canonical JSON bytes of this fact.
    pub(crate) fn json(&self) -> String {
        match self {
            Self::Card(card) => {
                let mut fields = vec![
                    (
                        "description",
                        match &card.description {
                            Some(text) => super::canonical::string(text),
                            None => "null".to_owned(),
                        },
                    ),
                    ("id", super::canonical::string(&card.qualified)),
                    ("kind", super::canonical::string(&card.kind)),
                ];
                fields.push((
                    "module",
                    match &card.module {
                        Some(module) => super::canonical::string(module),
                        None => "null".to_owned(),
                    },
                ));
                if let Some(version) = card.version {
                    fields.push(("version", version.to_string()));
                }
                if let Some(visibility) = card.visibility {
                    fields.push(("visibility", super::canonical::string(visibility)));
                }
                if let Some(portability) = card.portability {
                    fields.push(("portability", super::canonical::string(portability)));
                }
                if !card.derived_from.is_empty() {
                    fields.push((
                        "derivedFrom",
                        super::canonical::array(
                            card.derived_from
                                .iter()
                                .map(|id| super::canonical::string(id)),
                        ),
                    ));
                }
                if let Some(subkind) = &card.subkind {
                    fields.push(("subkind", super::canonical::string(subkind)));
                }
                if let Some(contract) = &card.contract {
                    fields.push(("contract", contract_json(contract)));
                }
                super::canonical::object(fields)
            }
            Self::Policy {
                id,
                decision,
                applies_to,
                description,
            } => {
                let mut fields = vec![
                    (
                        "appliesTo",
                        super::canonical::array(
                            applies_to.iter().map(|id| super::canonical::string(id)),
                        ),
                    ),
                    ("decision", super::canonical::string(decision)),
                    ("id", super::canonical::string(id)),
                ];
                if let Some(text) = description {
                    fields.push(("description", super::canonical::string(text)));
                }
                super::canonical::object(fields)
            }
            Self::Effect(effect) => {
                let mut fields = vec![
                    (
                        "action",
                        match effect.action {
                            Some(action) => super::canonical::string(action),
                            None => "null".to_owned(),
                        },
                    ),
                    ("confidence", super::canonical::string(effect.confidence)),
                    ("kind", super::canonical::string(&effect.kind)),
                    ("occurrence", effect.occurrence.to_string()),
                    ("operation", super::canonical::string(&effect.operation)),
                    (
                        "resource",
                        super::canonical::object(vec![
                            ("id", super::canonical::string(&effect.resource)),
                            ("kind", super::canonical::string(&effect.resource_kind)),
                        ]),
                    ),
                ];
                fields.push((
                    "field",
                    match &effect.field {
                        Some(field) => super::canonical::string(field),
                        None => "null".to_owned(),
                    },
                ));
                super::canonical::object(fields)
            }
            Self::Edge(edge) => super::canonical::object(vec![
                ("confidence", super::canonical::string(edge.confidence)),
                ("from", super::canonical::string(&edge.from)),
                ("occurrence", edge.occurrence.to_string()),
                ("relation", super::canonical::string(&edge.relation)),
                ("to", super::canonical::string(&edge.to)),
            ]),
            Self::Scenario {
                id,
                summary,
                covers,
            } => super::canonical::object(vec![
                (
                    "covers",
                    super::canonical::array(covers.iter().map(|id| super::canonical::string(id))),
                ),
                ("id", super::canonical::string(id)),
                ("summary", super::canonical::string(summary)),
            ]),
            Self::Binding { id, target } => super::canonical::object(vec![
                ("id", super::canonical::string(id)),
                ("target", super::canonical::string(target)),
            ]),
        }
    }

    /// The deterministic Markdown lines of this fact (the first line
    /// carries the fact id).
    pub(crate) fn markdown(&self) -> Vec<String> {
        match self {
            Self::Card(card) => {
                let mut lines = vec![format!("- {} ({})", card.qualified, card.kind)];
                if let Some(subkind) = &card.subkind {
                    lines.push(format!("  subkind: {subkind}"));
                }
                if let Some(module) = &card.module {
                    lines.push(format!("  module: {module}"));
                }
                if let Some(version) = card.version {
                    lines.push(format!("  version: {version}"));
                }
                if let Some(visibility) = card.visibility {
                    lines.push(format!("  visibility: {visibility}"));
                }
                if let Some(portability) = card.portability {
                    lines.push(format!("  portability: {portability}"));
                }
                if !card.derived_from.is_empty() {
                    lines.push(format!("  derived from: {}", card.derived_from.join(", ")));
                }
                if let Some(description) = &card.description {
                    lines.push(format!("  description: {description}"));
                }
                contract_markdown(&card.contract, &mut lines);
                lines
            }
            Self::Policy {
                id,
                decision,
                applies_to,
                description,
            } => {
                let mut lines = vec![format!("- {id} -> {decision}")];
                if !applies_to.is_empty() {
                    lines.push(format!("  applies to: {}", applies_to.join(", ")));
                }
                if let Some(text) = description {
                    lines.push(format!("  description: {text}"));
                }
                lines
            }
            Self::Effect(effect) => {
                let subject = match &effect.field {
                    Some(field) => format!("{}#{}", effect.resource, field),
                    None => effect.resource.clone(),
                };
                let action = match effect.action {
                    Some(action) => format!("[{action}] "),
                    None => String::new(),
                };
                vec![format!(
                    "- {} {}{} -> {}:{} ({})",
                    effect.kind,
                    action,
                    effect.operation,
                    effect.resource_kind,
                    subject,
                    effect.confidence
                )]
            }
            Self::Edge(edge) => vec![format!(
                "- {} {} -> {} ({})",
                edge.relation, edge.from, edge.to, edge.confidence
            )],
            Self::Scenario {
                id,
                summary,
                covers,
            } => {
                let mut lines = vec![format!("- {id}: {summary}")];
                if !covers.is_empty() {
                    lines.push(format!("  covers: {}", covers.join(", ")));
                }
                lines
            }
            Self::Binding { id, target } => {
                vec![format!("- {id} -> {target}")]
            }
        }
    }
}

/// The estimator content of one contract.
fn contract_content(contract: &Option<Contract>, parts: &mut Vec<String>) {
    let Some(contract) = contract else {
        return;
    };
    match contract {
        Contract::Scalar { base } => parts.push((*base).to_owned()),
        Contract::Enum { values } => {
            for (value, description) in values {
                parts.push(value.clone());
                parts.extend(description.clone());
            }
        }
        Contract::Fields { fields, identity } => {
            fields_content(fields, parts);
            parts.extend(identity.iter().cloned());
        }
        Contract::Command { input, effects } => {
            fields_content(input, parts);
            parts.extend(effects.iter().cloned());
        }
        Contract::Query { reads, returns } => {
            parts.extend(reads.iter().cloned());
            if let Some(returns) = returns {
                parts.push(returns.spell());
            }
        }
        Contract::Policy {
            applies_to,
            decision,
        } => {
            parts.push((*decision).to_owned());
            parts.extend(applies_to.iter().cloned());
        }
        Contract::Event { payload } => fields_content(payload, parts),
        Contract::Effect {
            operation,
            entity,
            emits,
        } => {
            parts.push((*operation).to_owned());
            parts.push(entity.clone());
            parts.extend(emits.iter().cloned());
        }
        Contract::Endpoint {
            invokes,
            method,
            path,
        } => {
            parts.push((*method).to_owned());
            parts.push(path.clone());
            parts.push(invokes.clone());
        }
        Contract::Scenario { covers, summary } => {
            parts.push(summary.clone());
            parts.extend(covers.iter().cloned());
        }
        Contract::Binding { target } => parts.push(target.clone()),
    }
}

fn fields_content(fields: &[WireField], parts: &mut Vec<String>) {
    for field in fields {
        parts.push(field.name.clone());
        if field.required {
            parts.push("required".to_owned());
        }
        parts.push(field.r#type.spell());
    }
}

/// The canonical JSON bytes of one contract.
fn contract_json(contract: &Contract) -> String {
    match contract {
        Contract::Scalar { base } => {
            super::canonical::object(vec![("base", super::canonical::string(base))])
        }
        Contract::Enum { values } => super::canonical::object(vec![(
            "values",
            super::canonical::array(values.iter().map(|(value, description)| {
                let mut fields = vec![("value", super::canonical::string(value))];
                if let Some(text) = description {
                    fields.push(("description", super::canonical::string(text)));
                }
                super::canonical::object(fields)
            })),
        )]),
        Contract::Fields { fields, identity } => {
            let mut fields_json = vec![("fields", fields_json(fields))];
            if !identity.is_empty() {
                fields_json.push((
                    "identity",
                    super::canonical::array(
                        identity.iter().map(|name| super::canonical::string(name)),
                    ),
                ));
            }
            super::canonical::object(fields_json)
        }
        Contract::Command { input, effects } => super::canonical::object(vec![
            (
                "effects",
                super::canonical::array(effects.iter().map(|id| super::canonical::string(id))),
            ),
            ("input", fields_json(input)),
        ]),
        Contract::Query { reads, returns } => {
            let mut fields = vec![(
                "reads",
                super::canonical::array(reads.iter().map(|id| super::canonical::string(id))),
            )];
            fields.push((
                "returns",
                match returns {
                    Some(returns) => type_json(returns),
                    None => "null".to_owned(),
                },
            ));
            super::canonical::object(fields)
        }
        Contract::Policy {
            applies_to,
            decision,
        } => super::canonical::object(vec![
            (
                "appliesTo",
                super::canonical::array(applies_to.iter().map(|id| super::canonical::string(id))),
            ),
            ("decision", super::canonical::string(decision)),
        ]),
        Contract::Event { payload } => {
            super::canonical::object(vec![("payload", fields_json(payload))])
        }
        Contract::Effect {
            operation,
            entity,
            emits,
        } => super::canonical::object(vec![
            (
                "emits",
                super::canonical::array(emits.iter().map(|id| super::canonical::string(id))),
            ),
            ("entity", super::canonical::string(entity)),
            ("operation", super::canonical::string(operation)),
        ]),
        Contract::Endpoint {
            invokes,
            method,
            path,
        } => super::canonical::object(vec![
            ("invokes", super::canonical::string(invokes)),
            ("method", super::canonical::string(method)),
            ("path", super::canonical::string(path)),
        ]),
        Contract::Scenario { covers, summary } => super::canonical::object(vec![
            (
                "covers",
                super::canonical::array(covers.iter().map(|id| super::canonical::string(id))),
            ),
            ("summary", super::canonical::string(summary)),
        ]),
        Contract::Binding { target } => {
            super::canonical::object(vec![("target", super::canonical::string(target))])
        }
    }
}

fn fields_json(fields: &[WireField]) -> String {
    super::canonical::array(fields.iter().map(|field| {
        super::canonical::object(vec![
            ("name", super::canonical::string(&field.name)),
            ("required", super::canonical::boolean(field.required)),
            ("type", type_json(&field.r#type)),
        ])
    }))
}

fn type_json(r#type: &WireType) -> String {
    match r#type {
        WireType::Ref(id) => super::canonical::object(vec![("ref", super::canonical::string(id))]),
        WireType::List(inner) => super::canonical::object(vec![("list", type_json(inner))]),
        WireType::Optional(inner) => super::canonical::object(vec![("optional", type_json(inner))]),
    }
}

/// The deterministic Markdown lines of one contract.
fn contract_markdown(contract: &Option<Contract>, lines: &mut Vec<String>) {
    let Some(contract) = contract else {
        return;
    };
    match contract {
        Contract::Scalar { base } => lines.push(format!("  base: {base}")),
        Contract::Enum { values } => {
            for (value, description) in values {
                match description {
                    Some(text) => lines.push(format!("  value: {value} ({text})")),
                    None => lines.push(format!("  value: {value}")),
                }
            }
        }
        Contract::Fields { fields, identity } => {
            fields_markdown("field", fields, lines);
            if !identity.is_empty() {
                lines.push(format!("  identity: {}", identity.join(", ")));
            }
        }
        Contract::Command { input, effects } => {
            fields_markdown("input", input, lines);
            if !effects.is_empty() {
                lines.push(format!("  effects: {}", effects.join(", ")));
            }
        }
        Contract::Query { reads, returns } => {
            if !reads.is_empty() {
                lines.push(format!("  reads: {}", reads.join(", ")));
            }
            if let Some(returns) = returns {
                lines.push(format!("  returns: {}", returns.spell()));
            }
        }
        Contract::Policy {
            applies_to,
            decision,
        } => {
            lines.push(format!("  decision: {decision}"));
            if !applies_to.is_empty() {
                lines.push(format!("  applies to: {}", applies_to.join(", ")));
            }
        }
        Contract::Event { payload } => fields_markdown("payload", payload, lines),
        Contract::Effect {
            operation,
            entity,
            emits,
        } => {
            lines.push(format!("  operation: {operation}"));
            lines.push(format!("  entity: {entity}"));
            if !emits.is_empty() {
                lines.push(format!("  emits: {}", emits.join(", ")));
            }
        }
        Contract::Endpoint {
            invokes,
            method,
            path,
        } => {
            lines.push(format!("  endpoint: {method} {path}"));
            lines.push(format!("  invokes: {invokes}"));
        }
        Contract::Scenario { covers, summary } => {
            lines.push(format!("  summary: {summary}"));
            if !covers.is_empty() {
                lines.push(format!("  covers: {}", covers.join(", ")));
            }
        }
        Contract::Binding { target } => lines.push(format!("  target: {target}")),
    }
}

fn fields_markdown(label: &str, fields: &[WireField], lines: &mut Vec<String>) {
    for field in fields {
        let required = if field.required { " (required)" } else { "" };
        lines.push(format!(
            "  {label} {}: {}{}",
            field.name,
            field.r#type.spell(),
            required
        ));
    }
}

/// One manifest row's canonical JSON bytes.
pub(crate) fn manifest_row_json(row: &ManifestRow) -> String {
    let mut fields = vec![
        ("id", super::canonical::string(&row.id)),
        ("section", super::canonical::string(row.section)),
    ];
    match row.reason {
        Some(reason) => fields.push(("reason", super::canonical::string(reason.as_str()))),
        None => fields.push(("tokens", row.tokens.to_string())),
    }
    super::canonical::object(fields)
}

/// One gap row's canonical JSON bytes.
pub(crate) fn gap_json(gap: &Gap) -> String {
    let mut fields = vec![("gap", super::canonical::string(gap.gap))];
    if !gap.symbols.is_empty() {
        fields.push((
            "symbols",
            super::canonical::array(
                gap.symbols
                    .iter()
                    .map(|symbol| super::canonical::string(&bound_token(symbol))),
            ),
        ));
    }
    super::canonical::object(fields)
}
