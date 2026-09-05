//! Canonical serialization of the inspect result (issue #15).
//!
//! Compact UTF-8 JSON, no insignificant whitespace, no trailing
//! newline. Top-level fields and sections appear in the fixed wire
//! order of `contracts/inspect.schema.v1.0.0.json`; the members of
//! set-like arrays are sorted upstream by unsigned UTF-8 bytes of their
//! typed ids, and per-item object keys follow the schema order. The
//! bytes are path-independent: no physical root, raw source text,
//! timestamp, host, locale, runtime value, or adapter transcript ever
//! enters them. The CLI adds exactly one trailing LF; these functions
//! never do.

use super::result::{
    BindingItem, Bounds, Completeness, ContractBody, ContractSection, InspectResult, Item,
    ItemsSection, PortabilitySection, ProjectHeader, SectionState, SelectorProjection, SourceRef,
    SymbolCard, TypeRefProjection, UnsupportedSection,
};
use crate::loader::canonical::write_json_string;

/// Serialize the whole inspect result to canonical payload bytes.
pub(crate) fn payload_bytes(result: &InspectResult) -> String {
    let mut json = String::from("{\"schemaVersion\":");
    json.push_str(&quote("lekalo/inspect/v1.0.0"));
    json.push_str(",\"identity\":");
    json.push_str(&quote(super::version::IDENTITY));
    json.push_str(",\"modelVersion\":");
    json.push_str(&quote(&result.model_version));
    json.push_str(",\"project\":");
    json.push_str(&project_bytes(&result.project));
    json.push_str(",\"selector\":");
    json.push_str(&selector_bytes(&result.selector));
    json.push_str(",\"symbol\":");
    json.push_str(&symbol_bytes(&result.symbol));
    json.push_str(",\"contract\":");
    json.push_str(&contract_bytes(&result.contract));
    for (key, section) in [
        ("invariants", &result.invariants),
        ("policies", &result.policies),
        ("effects", &result.effects),
        ("dependencies", &result.dependencies),
        ("dependents", &result.dependents),
        ("scenarios", &result.scenarios),
        ("bindings", &result.bindings),
    ] {
        json.push(',');
        json.push_str(&quote(key));
        json.push(':');
        json.push_str(&items_bytes(section));
    }
    json.push_str(",\"ownership\":");
    json.push_str(&unsupported_bytes(&result.ownership));
    json.push_str(",\"portability\":");
    json.push_str(&portability_bytes(&result.portability));
    json.push_str(",\"trace\":");
    json.push_str(&unsupported_bytes(&result.trace));
    json.push_str(",\"completeness\":");
    json.push_str(&completeness_bytes(&result.completeness));
    json.push_str(",\"diagnostics\":[]}");
    json
}

/// The project header bytes.
fn project_bytes(project: &Option<ProjectHeader>) -> String {
    let Some(project) = project else {
        return String::from("null");
    };
    format!(
        "{{\"id\":{},\"irRef\":{{\"digest\":{},\"identity\":{}}},\"modelRef\":{}}}",
        quote(&project.id),
        quote(&project.ir_digest),
        quote(&project.ir_identity),
        quote(&project.model_ref),
    )
}

/// The selector projection bytes.
fn selector_bytes(selector: &SelectorProjection) -> String {
    format!(
        "{{\"input\":{},\"mode\":{},\"resolvedId\":{}}}",
        quote(&selector.input),
        quote(selector.mode),
        quote(&selector.resolved_id),
    )
}

/// The symbol identity card bytes.
fn symbol_bytes(symbol: &SymbolCard) -> String {
    let mut json = format!(
        "{{\"id\":{},\"kind\":{},\"moduleId\":{},\"version\":{}",
        quote(&symbol.id),
        quote(symbol.kind),
        quote(&symbol.module_id),
        symbol.version,
    );
    if let Some(description) = &symbol.description {
        json.push_str(",\"description\":");
        json.push_str(&quote(description));
    }
    if let Some(visibility) = symbol.visibility {
        json.push_str(",\"visibility\":");
        json.push_str(&quote(visibility));
    }
    if let Some(portability) = symbol.portability {
        json.push_str(",\"portability\":");
        json.push_str(&quote(portability));
    }
    if let Some(source) = &symbol.source {
        json.push_str(",\"source\":");
        json.push_str(&source_bytes(source));
    }
    json.push('}');
    json
}

/// The logical source location bytes.
fn source_bytes(source: &SourceRef) -> String {
    format!(
        "{{\"path\":{},\"pointer\":{},\"span\":{{\"endByte\":{},\"startByte\":{}}}}}",
        quote(&source.path),
        quote(&source.pointer),
        source.end_byte,
        source.start_byte,
    )
}

/// The contract section bytes: section envelope first, then the
/// per-kind body keys in schema order.
fn contract_bytes(contract: &ContractSection) -> String {
    let mut json = format!(
        "{{\"complete\":{},\"state\":{}",
        contract.complete,
        quote(contract.state.as_str()),
    );
    if let Some(bounds) = &contract.bounds {
        json.push_str(",\"bounds\":");
        json.push_str(&bounds_bytes(bounds));
    }
    match &contract.body {
        ContractBody::Scalar { base } => {
            json.push_str(&format!(",\"base\":{}", quote(base)));
        }
        ContractBody::Enum { values } => {
            json.push_str(",\"values\":[");
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    json.push(',');
                }
                json.push_str(&format!("{{\"value\":{}}}", quote(&value.value)));
                if let Some(description) = &value.description {
                    json.push_str(&format!(",\"description\":{}", quote(description)));
                }
            }
            json.push(']');
        }
        ContractBody::Fields { fields } => {
            json.push_str(",\"fields\":");
            json.push_str(&fields_bytes(fields));
        }
        ContractBody::Command { input } => {
            json.push_str(",\"input\":");
            json.push_str(&fields_bytes(input));
        }
        ContractBody::Query { reads, output } => {
            json.push_str(",\"reads\":");
            json.push_str(&string_array(reads));
            if let Some(output) = output {
                json.push_str(",\"output\":");
                json.push_str(&type_ref_bytes(output));
            }
        }
        ContractBody::Policy { decision } => {
            json.push_str(&format!(",\"decision\":{}", quote(decision)));
        }
        ContractBody::Event { payload } => {
            json.push_str(",\"fields\":");
            json.push_str(&fields_bytes(payload));
        }
        ContractBody::Effect {
            operation,
            target,
            emits,
        } => {
            json.push_str(&format!(
                ",\"operation\":{},\"target\":{},\"emits\":{}",
                quote(operation),
                quote(target),
                string_array(emits),
            ));
        }
        ContractBody::Endpoint {
            invokes,
            method,
            path,
        } => {
            json.push_str(&format!(
                ",\"invokes\":{},\"method\":{},\"path\":{}",
                quote(invokes),
                quote(method),
                quote(path),
            ));
        }
        ContractBody::Scenario { summary, covers } => {
            json.push_str(&format!(
                ",\"summary\":{},\"covers\":{}",
                quote(summary),
                string_array(covers),
            ));
        }
        ContractBody::TargetBinding { target_name } => {
            json.push_str(&format!(",\"targetName\":{}", quote(target_name)));
        }
    }
    json.push('}');
    json
}

/// The bounded field array bytes.
fn fields_bytes(fields: &[super::result::FieldProjection]) -> String {
    let mut json = String::from("[");
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            "{{\"name\":{},\"type\":{},\"required\":{}}}",
            quote(&field.name),
            type_ref_bytes(&field.r#type),
            field.required,
        ));
        if let Some(description) = &field.description {
            json.push_str(&format!(",\"description\":{}", quote(description)));
        }
    }
    json.push(']');
    json
}

/// The named TypeRef bytes.
fn type_ref_bytes(reference: &TypeRefProjection) -> String {
    match reference {
        TypeRefProjection::Ref(id) => format!("{{\"ref\":{}}}", quote(id)),
        TypeRefProjection::List(inner) => {
            format!("{{\"list\":{}}}", type_ref_bytes(inner))
        }
        TypeRefProjection::Optional(inner) => {
            format!("{{\"optional\":{}}}", type_ref_bytes(inner))
        }
    }
}

/// The bounded string array bytes.
fn string_array(values: &[String]) -> String {
    let members: Vec<String> = values.iter().map(|value| quote(value)).collect();
    format!("[{}]", members.join(","))
}

/// One bounded section: state, complete, bounds, summary, items.
fn items_bytes(section: &ItemsSection) -> String {
    let mut json = format!(
        "{{\"complete\":{},\"state\":{}",
        section.complete,
        quote(section.state.as_str()),
    );
    if let Some(bounds) = &section.bounds {
        json.push_str(",\"bounds\":");
        json.push_str(&bounds_bytes(bounds));
    }
    if let Some(summary) = &section.summary {
        json.push_str(&format!(
            ",\"summary\":{{\"emitterCount\":{},\"readerCount\":{},\"writerCount\":{}}}",
            summary.emitter_count, summary.reader_count, summary.writer_count,
        ));
    }
    if !section.items.is_empty() {
        json.push_str(",\"items\":[");
        for (index, item) in section.items.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str(&item_bytes(item));
        }
        json.push(']');
    }
    json.push('}');
    json
}

/// One section item's bytes.
fn item_bytes(item: &Item) -> String {
    match item {
        Item::Invariant(invariant) => format!(
            "{{\"fields\":{},\"type\":\"identity\"}}",
            string_array(&invariant.fields),
        ),
        Item::Policy(policy) => format!(
            "{{\"decision\":{},\"id\":{}}}",
            quote(policy.decision),
            quote(&policy.id),
        ),
        Item::Effect(effect) => effect.json.clone(),
        Item::Relation(relation) => format!(
            "{{\"confidence\":{},\"endpoint\":{},\"occurrence\":{},\"provenance\":{},\"relation\":{}}}",
            quote(relation.confidence),
            quote(&relation.endpoint),
            relation.occurrence,
            relation.provenance_json,
            quote(relation.relation),
        ),
        Item::Scenario(scenario) => {
            let mut json = format!("{{\"id\":{}", quote(&scenario.id));
            if let Some(summary) = &scenario.summary {
                json.push_str(&format!(",\"summary\":{}", quote(summary)));
            }
            json.push('}');
            json
        }
        Item::Binding(binding) => binding_bytes(binding),
    }
}

/// One binding item's bytes.
fn binding_bytes(binding: &BindingItem) -> String {
    format!(
        "{{\"bindingId\":{},\"confidence\":\"canonical\",\"moduleId\":{},\"status\":\"declared\",\"targetId\":{}}}",
        quote(&binding.binding_id),
        quote(&binding.module_id),
        quote(&binding.target_id),
    )
}

/// The bounds record bytes.
fn bounds_bytes(bounds: &Bounds) -> String {
    let mut json = format!(
        "{{\"limit\":{},\"omitted\":{},\"returned\":{}",
        bounds.limit, bounds.omitted, bounds.returned,
    );
    if let Some(reason) = bounds.reason {
        json.push_str(",\"reason\":");
        json.push_str(&quote(reason));
    }
    if let Some(frontier) = &bounds.frontier {
        json.push_str(",\"frontier\":");
        json.push_str(&quote(frontier));
    }
    json.push('}');
    json
}

/// The unsupported section bytes.
fn unsupported_bytes(section: &UnsupportedSection) -> String {
    format!(
        "{{\"complete\":false,\"reason\":{},\"state\":{}}}",
        quote(section.reason),
        quote(section.state.as_str()),
    )
}

/// The portability section bytes.
fn portability_bytes(section: &PortabilitySection) -> String {
    let mut json = format!(
        "{{\"complete\":{},\"state\":{}",
        section.state != SectionState::Unknown,
        quote(section.state.as_str()),
    );
    if let Some(mode) = section.mode {
        json.push_str(",\"mode\":");
        json.push_str(&quote(mode));
    }
    json.push('}');
    json
}

/// The completeness marker bytes.
fn completeness_bytes(completeness: &Completeness) -> String {
    format!(
        "{{\"omittedItems\":{},\"reasons\":{},\"state\":{}}}",
        completeness.omitted_items,
        string_array(
            &completeness
                .reasons
                .iter()
                .map(|reason| (*reason).to_owned())
                .collect::<Vec<String>>()
        ),
        quote(completeness.state),
    )
}

/// Quote one JSON string.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    write_json_string(text, &mut out);
    out
}
