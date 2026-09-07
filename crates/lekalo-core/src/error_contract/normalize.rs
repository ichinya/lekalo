//! Canonical serialization of the error registry and operation bindings
//! (issue #62).
//!
//! Compact UTF-8 JSON, no insignificant whitespace, object keys in
//! unsigned UTF-8 byte order, records in canonical order, no final line
//! feed. The bytes are path-independent: no physical root, raw source,
//! timestamp, host, locale, or runtime value ever enters them.

use super::registry::ErrorRegistry;
use super::result::{FieldValue, PublicPayload};
use super::types::{
    Coverage, ErrorContract, ErrorPayload, Messages, OperationErrorContract, OutputType,
    RetryPolicy, SourceSpan, TypeExpr, Waiver,
};

/// Quote one string into canonical JSON bytes.
fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

/// The canonical bytes of one registry document.
pub fn registry_bytes(
    registry: &ErrorRegistry,
) -> Result<String, crate::diagnostics::DiagnosticSet> {
    let bytes = render(registry);
    if bytes.len() > super::version::MAX_EXPORT_BYTES {
        return Err(super::diagnostic::limit_exceeded_set(
            "export-bound",
            "33554432",
        ));
    }
    Ok(bytes)
}

fn render(registry: &ErrorRegistry) -> String {
    let mut json = String::from("{\"bindings\":[");
    for (index, binding) in registry.bindings().iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&binding_bytes(binding));
    }
    json.push_str("],\"closed\":true,\"errors\":[");
    for (index, error) in registry.errors().iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&error_bytes(error));
    }
    json.push_str("],\"identity\":");
    json.push_str(&quote(super::version::REGISTRY_IDENTITY));
    json.push_str(",\"schema_version\":");
    json.push_str(&quote(super::version::REGISTRY_SCHEMA_VERSION));
    json.push_str(",\"tombstones\":[");
    for (index, tombstone) in registry.tombstones().iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str("{\"code\":");
        json.push_str(&quote(tombstone.code.as_str()));
        json.push_str(",\"reason\":");
        json.push_str(&quote(&tombstone.reason));
        json.push('}');
    }
    json.push_str("]}");
    json
}

/// The canonical bytes of one operation binding.
pub fn binding_bytes(binding: &OperationErrorContract) -> String {
    let mut json = String::from("{\"errors\":[");
    for (index, member) in binding.errors().members().iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&quote(member.id().as_str()));
    }
    json.push_str("],\"kind\":");
    json.push_str(&quote(binding.kind().as_str()));
    json.push_str(",\"operation\":");
    json.push_str(&quote(binding.operation().as_str()));
    json.push_str(",\"output\":");
    json.push_str(&output_bytes(binding.output()));
    json.push('}');
    json
}

fn output_bytes(output: &OutputType) -> String {
    match output {
        OutputType::Unit => "null".to_owned(),
        OutputType::Value(expr) => type_bytes(expr),
    }
}

fn type_bytes(expr: &TypeExpr) -> String {
    match expr {
        TypeExpr::Ref(id) => format!("{{\"ref\":{}}}", quote(id.as_str())),
        TypeExpr::List(inner) => format!("{{\"list\":{}}}", type_bytes(inner)),
        TypeExpr::Optional(inner) => format!("{{\"optional\":{}}}", type_bytes(inner)),
    }
}

/// The canonical bytes of one error contract.
pub fn error_bytes(error: &ErrorContract) -> String {
    let mut json = String::from("{\"category\":");
    json.push_str(&quote(error.category().as_str()));
    json.push_str(",\"code\":");
    json.push_str(&quote(error.code().as_str()));
    json.push_str(",\"coverage\":");
    json.push_str(&coverage_bytes(error.coverage()));
    json.push_str(",\"effect\":");
    json.push_str(&quote(error.effect().as_str()));
    json.push_str(",\"id\":");
    json.push_str(&quote(error.id().as_str()));
    json.push_str(",\"idempotency\":");
    json.push_str(&quote(error.idempotency().as_str()));
    json.push_str(",\"invariant\":");
    json.push_str(&quote(error.invariant()));
    json.push_str(",\"messages\":");
    json.push_str(&messages_bytes(error.messages()));
    json.push_str(",\"observability\":");
    json.push_str(&quote(error.observability().as_str()));
    json.push_str(",\"payload\":");
    json.push_str(&payload_bytes(error.payload()));
    json.push_str(",\"retry\":");
    json.push_str(&retry_bytes(error.retry()));
    json.push_str(",\"source\":");
    json.push_str(&source_bytes(error.source()));
    json.push('}');
    json
}

fn coverage_bytes(coverage: &Coverage) -> String {
    match coverage {
        Coverage::Scenarios(refs) => {
            let mut json = String::from("{\"scenarios\":[");
            for (index, reference) in refs.iter().enumerate() {
                if index > 0 {
                    json.push(',');
                }
                json.push_str(&quote(reference.as_str()));
            }
            json.push_str("]}");
            json
        }
        Coverage::Tests(refs) => {
            let mut json = String::from("{\"tests\":[");
            for (index, reference) in refs.iter().enumerate() {
                if index > 0 {
                    json.push(',');
                }
                json.push_str(&quote(reference.as_str()));
            }
            json.push_str("]}");
            json
        }
        Coverage::Waiver(waiver) => waiver_bytes(waiver),
    }
}

fn waiver_bytes(waiver: &Waiver) -> String {
    let mut json = String::from("{\"waiver\":{");
    if let Some(expires) = waiver.expires() {
        json.push_str("\"expires\":");
        json.push_str(&quote(expires));
        json.push(',');
    }
    json.push_str("\"owner\":");
    json.push_str(&quote(waiver.owner()));
    json.push_str(",\"reason\":");
    json.push_str(&quote(waiver.reason()));
    json.push_str(",\"ref\":");
    json.push_str(&quote(waiver.reference()));
    json.push_str("}}");
    json
}

fn messages_bytes(messages: &Messages) -> String {
    let mut json = String::from("{");
    if let Some(private) = messages.private() {
        json.push_str("\"private\":");
        json.push_str(&template_bytes(private));
        json.push(',');
    }
    json.push_str("\"public\":");
    json.push_str(&template_bytes(messages.public()));
    json.push('}');
    json
}

fn template_bytes(template: &super::types::MessageTemplate) -> String {
    let mut json = String::from("{\"fields\":[");
    for (index, field) in template.fields().iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&quote(field));
    }
    json.push_str("],\"template\":");
    json.push_str(&quote(template.template()));
    json.push('}');
    json
}

fn payload_bytes(payload: &ErrorPayload) -> String {
    let mut json = String::from("{\"fields\":[");
    for (index, field) in payload.fields().iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str("{\"exposure\":");
        json.push_str(&quote(field.exposure().as_str()));
        json.push_str(",\"name\":");
        json.push_str(&quote(field.name()));
        json.push_str(",\"required\":");
        json.push_str(if field.required() { "true" } else { "false" });
        json.push_str(",\"type\":");
        json.push_str(&type_bytes(field.field_type()));
        json.push('}');
    }
    json.push_str("]}");
    json
}

fn retry_bytes(retry: RetryPolicy) -> String {
    match retry {
        RetryPolicy::Never => "{\"policy\":\"never\"}".to_owned(),
        RetryPolicy::Safe => "{\"policy\":\"safe\"}".to_owned(),
        RetryPolicy::Conditional(condition) => format!(
            "{{\"condition\":{},\"policy\":\"conditional\"}}",
            quote(condition.as_str())
        ),
    }
}

fn source_bytes(source: &SourceSpan) -> String {
    let start = source.start();
    let end = source.end();
    format!(
        "{{\"end\":{{\"byte\":{},\"column\":{},\"line\":{}}},\"path\":{},\"start\":{{\"byte\":{},\"column\":{},\"line\":{}}}}}",
        end.byte,
        end.column,
        end.line,
        quote(source.path()),
        start.byte,
        start.column,
        start.line,
    )
}

/// The canonical public payload projection: `{"field":value}` with only
/// public values, byte-sorted by field name.
pub fn public_payload_bytes(payload: &PublicPayload) -> String {
    let mut json = String::from("{");
    for (index, (name, value)) in payload.values().iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&quote(name));
        json.push(':');
        json.push_str(&value_bytes(value));
    }
    json.push('}');
    json
}

fn value_bytes(value: &FieldValue) -> String {
    match value {
        FieldValue::Text(text) => quote(text),
        FieldValue::Count(count) => count.to_string(),
        FieldValue::Flag(flag) => flag.to_string(),
        FieldValue::TextList(values) => {
            let mut json = String::from("[");
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    json.push(',');
                }
                json.push_str(&quote(value));
            }
            json.push(']');
            json
        }
    }
}

/// The canonical language-neutral core quadruple every projection must
/// preserve: `{"category":..,"code":..,"id":..,"payload":{..}}`.
pub fn identity_bytes(error: &ErrorContract, payload: &PublicPayload) -> String {
    format!(
        "{{\"category\":{},\"code\":{},\"id\":{},\"payload\":{}}}",
        quote(error.category().as_str()),
        quote(error.code().as_str()),
        quote(error.id().as_str()),
        public_payload_bytes(payload),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_keys_render_in_byte_order() {
        let rendered = source_bytes(&SourceSpan {
            path: "a/b.yaml".to_owned(),
            start: super::super::types::Position {
                byte: 1,
                line: 1,
                column: 2,
            },
            end: super::super::types::Position {
                byte: 3,
                line: 1,
                column: 4,
            },
        });
        assert_eq!(
            rendered,
            "{\"end\":{\"byte\":3,\"column\":4,\"line\":1},\"path\":\"a/b.yaml\",\"start\":{\"byte\":1,\"column\":2,\"line\":1}}"
        );
    }
}
