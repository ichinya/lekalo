//! The inspect result model: one normalized object projected into both
//! wire bytes and the fixed human view (issue #15).
//! Every section is present in every result: a section whose data
//! source is missing is explicitly `unsupported` with a reason, a
//! section beyond a bound is `truncated` with returned/omitted counts
//! and the deterministic frontier, and a valid symbol with no facts in
//! one section is `empty` — never an absent key or a silent omission.
//! Top-level fields and sections follow the fixed wire order of
//! `contracts/inspect.schema.v1.0.0.json`; set-like arrays are sorted
//! by unsigned UTF-8 bytes of their typed ids.

/// One finished inspect invocation: canonical payload bytes plus the
/// human view of the same normalized object.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectOutcome {
    /// The canonical inspect payload bytes (no envelope, no trailing
    /// newline).
    pub json: String,
    /// The fixed human view of the same result.
    pub human: String,
}

/// The closed section state vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SectionState {
    Available,
    Empty,
    Unknown,
    Unsupported,
    Truncated,
}

impl SectionState {
    /// The exact wire spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Empty => "empty",
            Self::Unknown => "unknown",
            Self::Unsupported => "unsupported",
            Self::Truncated => "truncated",
        }
    }
}

/// The returned/omitted bookkeeping of one bounded section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Bounds {
    pub limit: usize,
    pub returned: usize,
    pub omitted: usize,
    pub reason: Option<&'static str>,
    /// The sort key of the first omitted item, when a deterministic
    /// continuation exists.
    pub frontier: Option<String>,
}

/// The whole-result completeness marker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Completeness {
    pub state: &'static str,
    /// Fixed vocabulary tokens, in a fixed order.
    pub reasons: Vec<&'static str>,
    pub omitted_items: usize,
}

/// The project header: identity and contract references only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectHeader {
    pub id: String,
    /// `dev.lekalo.model@<version>`.
    pub model_ref: String,
    /// `dev.lekalo.ir@<version>`.
    pub ir_identity: String,
    /// The `sha256` digest of the canonical IR bytes.
    pub ir_digest: String,
}

/// The resolved selector, echoed only after the grammar accepted it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SelectorProjection {
    pub mode: &'static str,
    pub input: String,
    pub resolved_id: String,
}

/// The identity card of the resolved symbol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SymbolCard {
    pub id: String,
    pub kind: &'static str,
    pub module_id: String,
    pub version: u64,
    pub description: Option<String>,
    pub visibility: Option<&'static str>,
    pub portability: Option<&'static str>,
    pub source: Option<SourceRef>,
}

/// The logical source location: project-relative path, JSON pointer,
/// and byte span. Never a physical root or absolute path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceRef {
    pub path: String,
    pub pointer: String,
    pub start_byte: usize,
    pub end_byte: usize,
}

/// The named TypeRef projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TypeRefProjection {
    Ref(String),
    List(Box<TypeRefProjection>),
    Optional(Box<TypeRefProjection>),
}

/// One field projection with its named type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FieldProjection {
    pub name: String,
    pub r#type: TypeRefProjection,
    pub required: bool,
    pub description: Option<String>,
}

/// One enum member projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EnumValueProjection {
    pub value: String,
    pub description: Option<String>,
}

/// The per-kind contract body; the closed key set per definition kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ContractBody {
    Scalar {
        base: &'static str,
    },
    Enum {
        values: Vec<EnumValueProjection>,
    },
    Fields {
        fields: Vec<FieldProjection>,
    },
    Command {
        input: Vec<FieldProjection>,
    },
    Query {
        reads: Vec<String>,
        output: Option<TypeRefProjection>,
    },
    Policy {
        decision: &'static str,
    },
    Event {
        payload: Vec<FieldProjection>,
    },
    Effect {
        operation: &'static str,
        target: String,
        emits: Vec<String>,
    },
    Endpoint {
        invokes: String,
        method: &'static str,
        path: String,
    },
    Scenario {
        summary: String,
        covers: Vec<String>,
    },
    TargetBinding {
        target_name: String,
    },
}

/// The contract section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContractSection {
    pub state: SectionState,
    pub complete: bool,
    pub bounds: Option<Bounds>,
    pub body: ContractBody,
}

/// One entity identity invariant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InvariantItem {
    pub fields: Vec<String>,
}

/// One applicable policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PolicyItem {
    pub id: String,
    pub decision: &'static str,
}

/// One effect edge: the accepted #14 wire bytes plus its human line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EffectItem {
    pub json: String,
    pub human: String,
    pub declared: bool,
}

/// The read/write/emit counts of one entity or event resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EffectCounts {
    pub reader_count: usize,
    pub writer_count: usize,
    pub emitter_count: usize,
}

/// One direct graph relation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RelationItem {
    pub relation: &'static str,
    pub endpoint: String,
    pub occurrence: u32,
    /// The exact #13 provenance record bytes.
    pub provenance_json: String,
    pub confidence: &'static str,
}

/// One scenario that covers the symbol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScenarioItem {
    pub id: String,
    pub summary: Option<String>,
}

/// One declared target binding of the symbol's module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BindingItem {
    pub binding_id: String,
    pub module_id: String,
    pub target_id: String,
}

/// The closed per-item type of one items section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Item {
    Invariant(InvariantItem),
    Policy(PolicyItem),
    Effect(EffectItem),
    Relation(RelationItem),
    Scenario(ScenarioItem),
    Binding(BindingItem),
}

/// One bounded items section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ItemsSection {
    pub state: SectionState,
    pub complete: bool,
    pub bounds: Option<Bounds>,
    pub summary: Option<EffectCounts>,
    pub items: Vec<Item>,
}

/// The ownership and trace sections: typed optional projections whose
/// accepted owners (#21/#22) do not exist yet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnsupportedSection {
    pub state: SectionState,
    pub reason: &'static str,
}

/// The portability section: the declared declaration-level marker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PortabilitySection {
    pub state: SectionState,
    pub mode: Option<&'static str>,
}

/// The complete inspect result: every mandatory section, in wire order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InspectResult {
    pub model_version: String,
    pub project: Option<ProjectHeader>,
    pub selector: SelectorProjection,
    pub symbol: SymbolCard,
    pub contract: ContractSection,
    pub invariants: ItemsSection,
    pub policies: ItemsSection,
    pub effects: ItemsSection,
    pub dependencies: ItemsSection,
    pub dependents: ItemsSection,
    pub scenarios: ItemsSection,
    pub bindings: ItemsSection,
    pub ownership: UnsupportedSection,
    pub portability: PortabilitySection,
    pub trace: UnsupportedSection,
    pub completeness: Completeness,
}

impl InspectResult {
    /// The fixed human view: same object, fixed labels, no ANSI, no
    /// locale, stable section order.
    pub fn to_human(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("symbol: {}", self.symbol.id));
        lines.push(format!("kind: {}", self.symbol.kind));
        lines.push(format!("module: {}", self.symbol.module_id));
        lines.push(format!("version: {}", self.symbol.version));
        if let Some(description) = &self.symbol.description {
            lines.push(format!("description: {description}"));
        }
        if let Some(visibility) = self.symbol.visibility {
            lines.push(format!("visibility: {visibility}"));
        }
        if let Some(source) = &self.symbol.source {
            lines.push(format!("source: {}#{}", source.path, source.pointer));
        }
        lines.push(format!(
            "selector: {} {}",
            self.selector.mode, self.selector.input
        ));
        if self.selector.input != self.selector.resolved_id {
            lines.push(format!("resolved: {}", self.selector.resolved_id));
        }
        lines.push(String::from("contract:"));
        push_contract(&mut lines, &self.contract.body);
        push_items(&mut lines, "invariants", &self.invariants);
        push_items(&mut lines, "policies", &self.policies);
        push_items(&mut lines, "effects", &self.effects);
        push_items(&mut lines, "dependencies", &self.dependencies);
        push_items(&mut lines, "dependents", &self.dependents);
        push_items(&mut lines, "scenarios", &self.scenarios);
        push_items(&mut lines, "bindings", &self.bindings);
        lines.push(format!(
            "ownership: {} ({})",
            self.ownership.state.as_str(),
            self.ownership.reason
        ));
        match self.portability.mode {
            Some(mode) => lines.push(format!("portability: {mode}")),
            None => lines.push(format!("portability: {}", self.portability.state.as_str())),
        }
        lines.push(format!(
            "trace: {} ({})",
            self.trace.state.as_str(),
            self.trace.reason
        ));
        lines.push(format!("completeness: {}", self.completeness.state));
        if !self.completeness.reasons.is_empty() {
            lines.push(format!(
                "  reasons: {}",
                self.completeness.reasons.join(", ")
            ));
        }
        lines.join("\n")
    }
}

/// The contract body lines: one fixed `key: value` line per declared
/// fact, in the per-kind field order.
fn push_contract(lines: &mut Vec<String>, body: &ContractBody) {
    match body {
        ContractBody::Scalar { base } => lines.push(format!("  base: {base}")),
        ContractBody::Enum { values } => {
            for value in values {
                match &value.description {
                    Some(description) => {
                        lines.push(format!("  value: {} ({description})", value.value))
                    }
                    None => lines.push(format!("  value: {}", value.value)),
                }
            }
        }
        ContractBody::Fields { fields }
        | ContractBody::Command { input: fields }
        | ContractBody::Event { payload: fields } => {
            let label = match body {
                ContractBody::Command { .. } => "input",
                ContractBody::Event { .. } => "payload",
                _ => "fields",
            };
            lines.push(format!("  {label}:"));
            push_fields(lines, fields);
        }
        ContractBody::Query { reads, output } => {
            lines.push(format!("  reads: {}", reads.join(", ")));
            if let Some(output) = output {
                lines.push(format!("  returns: {}", type_ref_text(output)));
            }
        }
        ContractBody::Policy { decision } => lines.push(format!("  decision: {decision}")),
        ContractBody::Effect {
            operation,
            target,
            emits,
        } => {
            lines.push(format!("  operation: {operation}"));
            lines.push(format!("  entity: {target}"));
            lines.push(format!("  emits: {}", emits.join(", ")));
        }
        ContractBody::Endpoint {
            invokes,
            method,
            path,
        } => {
            lines.push(format!("  invokes: {invokes}"));
            lines.push(format!("  method: {method}"));
            lines.push(format!("  path: {path}"));
        }
        ContractBody::Scenario { summary, covers } => {
            lines.push(format!("  summary: {summary}"));
            lines.push(format!("  covers: {}", covers.join(", ")));
        }
        ContractBody::TargetBinding { target_name } => {
            lines.push(format!("  target: {target_name}"));
        }
    }
}

/// The field lines: `name: type [required]` in declaration order.
fn push_fields(lines: &mut Vec<String>, fields: &[FieldProjection]) {
    for field in fields {
        let requirement = if field.required { " [required]" } else { "" };
        match &field.description {
            Some(description) => lines.push(format!(
                "    - {}: {}{requirement} ({description})",
                field.name,
                type_ref_text(&field.r#type)
            )),
            None => lines.push(format!(
                "    - {}: {}{requirement}",
                field.name,
                type_ref_text(&field.r#type)
            )),
        }
    }
}

/// The Model surface spelling of one TypeRef projection.
fn type_ref_text(reference: &TypeRefProjection) -> String {
    match reference {
        TypeRefProjection::Ref(id) => id.clone(),
        TypeRefProjection::List(inner) => format!("list<{}>", type_ref_text(inner)),
        TypeRefProjection::Optional(inner) => format!("{}?", type_ref_text(inner)),
    }
}

/// One items section: the state line plus deterministic item lines.
fn push_items(lines: &mut Vec<String>, label: &str, section: &ItemsSection) {
    let mut line = format!("{label}: state={}", section.state.as_str());
    if let Some(summary) = &section.summary {
        line.push_str(&format!(
            " readers={} writers={} emitters={}",
            summary.reader_count, summary.writer_count, summary.emitter_count
        ));
    }
    if let Some(bounds) = &section.bounds {
        line.push_str(&format!(
            " returned={} omitted={}",
            bounds.returned, bounds.omitted
        ));
    }
    lines.push(line);
    for item in &section.items {
        match item {
            Item::Invariant(invariant) => {
                lines.push(format!("  - identity {}", invariant.fields.join(", ")))
            }
            Item::Policy(policy) => lines.push(format!("  - {} {}", policy.id, policy.decision)),
            Item::Effect(effect) => lines.push(format!("  - {}", effect.human)),
            Item::Relation(relation) => lines.push(format!(
                "  - {} {} ({})",
                relation.relation, relation.endpoint, relation.confidence
            )),
            Item::Scenario(scenario) => match &scenario.summary {
                Some(summary) => lines.push(format!("  - {} ({summary})", scenario.id)),
                None => lines.push(format!("  - {}", scenario.id)),
            },
            Item::Binding(binding) => lines.push(format!(
                "  - {} -> {} ({})",
                binding.binding_id, binding.target_id, binding.module_id
            )),
        }
    }
}
