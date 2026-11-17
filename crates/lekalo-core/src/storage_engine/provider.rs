//! Data-only bridge to the independently installed storage provider.
//! All SQL and capability answers come from the provider process.
use super::{diagnostic, Policies, StorageEngineAttachment};
use crate::diagnostics::DiagnosticSet;
use crate::storage_projection::derivation::{
    DerivedColumn, DerivedJoin, DerivedProjection, DerivedTable,
};
use crate::storage_projection::entity::{DomainType, FieldDefault, Literal};
use crate::storage_projection::projection::ColumnPredicate;
use crate::storage_projection::{Index, StorageProjectionAttachment};
use serde_json::{json, Value};

pub(crate) fn call(operation: &str, input: Value) -> Result<Value, DiagnosticSet> {
    let fail = |detail| diagnostic::rule_invalid(diagnostic::MAPPING_INVALID, detail, None);
    // Compatibility entry point for legacy storage commands. The value is a
    // Source (package reference or {command:[...]}), never shell text.
    let source = match std::env::var("LEKALO_STORAGE_PROVIDER") {
        Ok(value) => serde_json::from_str(&value).map_err(|_| fail("provider-config"))?,
        Err(_) => crate::runtime::config::Source::Executable(crate::runtime::config::Executable {
            command: vec!["lekalo-postgres".to_owned()],
        }),
    };
    let root = std::env::current_dir().map_err(|_| fail("provider-root"))?;
    // One command retains one provider admission; each exchange verifies the
    // original executable pins so concurrent upgrades cannot mix providers.
    thread_local! { static SESSION: std::cell::RefCell<Option<(String,crate::runtime::Session)>> = const { std::cell::RefCell::new(None) }; }
    let key = serde_json::to_string(&(root.to_string_lossy(), &source))
        .map_err(|_| fail("provider-config"))?;
    let response = SESSION.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.as_ref().is_none_or(|(current, _)| current != &key) {
            *slot = Some((
                key,
                crate::runtime::Session::new(&root, &source)
                    .map_err(|_| fail("provider-execution"))?,
            ));
        }
        slot.as_ref()
            .unwrap()
            .1
            .invoke(operation, input)
            .map_err(|_| fail("provider-execution"))
    })?;
    if let Some(error) = response.get("error") {
        let rule = if error["kind"] == "render" {
            diagnostic::RENDER_UNSUPPORTED
        } else {
            diagnostic::MAPPING_INVALID
        };
        return Err(diagnostic::rule_invalid(
            rule,
            error["detail"].as_str().unwrap_or("provider-refused"),
            None,
        ));
    }
    response
        .get("value")
        .cloned()
        .ok_or_else(|| fail("provider-response"))
}
pub(crate) fn text(operation: &str, input: Value) -> Result<String, DiagnosticSet> {
    call(operation, input)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| {
            diagnostic::rule_invalid(diagnostic::MAPPING_INVALID, "provider-response", None)
        })
}
pub(crate) fn policies(value: &Policies) -> Value {
    json!({"json":value.json().key(),"enum":value.enum_policy().key(),"array":value.array().key()})
}
pub(crate) fn domain_type(value: &DomainType) -> Value {
    match value {
        DomainType::Boolean => json!({"name":"boolean"}),
        DomainType::Integer => json!({"name":"integer"}),
        DomainType::Text => json!({"name":"text"}),
        DomainType::Uuid => json!({"name":"uuid"}),
        DomainType::Date => json!({"name":"date"}),
        DomainType::Timestamp => json!({"name":"timestamp"}),
        DomainType::Binary => json!({"name":"binary"}),
        DomainType::Json => json!({"name":"json"}),
        DomainType::Decimal { precision, scale } => {
            json!({"name":"decimal","precision":precision,"scale":scale})
        }
        DomainType::String { length } => json!({"name":"string","length":length}),
        DomainType::Enum { members } => json!({"name":"enum","members":members}),
        DomainType::Array { element, .. } => json!({"name":"array","element":domain_type(element)}),
    }
}
fn literal(value: &Literal) -> Value {
    match value {
        Literal::Boolean(v) => json!({"kind":"boolean","value":v}),
        Literal::Integer(v) => json!({"kind":"integer","value":v.to_string()}),
        Literal::Decimal(v) => json!({"kind":"decimal","value":v}),
        Literal::Text(v) => json!({"kind":"text","value":v}),
    }
}
pub(crate) fn default(value: &FieldDefault) -> Value {
    match value {
        FieldDefault::Literal(v) => json!({"kind":"literal","value":literal(v)}),
        FieldDefault::Now => json!({"kind":"now"}),
        FieldDefault::UuidGenerate => json!({"kind":"uuid_generate"}),
        FieldDefault::Sequence { column } => json!({"kind":"sequence","ref":column.as_str()}),
    }
}
fn predicates(rows: &[ColumnPredicate]) -> Value {
    json!(rows.iter().map(|p|json!({"column":p.column().as_str(),"op":p.op().key(),"value":p.value().map(literal)})).collect::<Vec<_>>())
}
pub(crate) fn index(value: &Index) -> Value {
    json!({"name":value.name().map(|n|n.as_str()),"columns":value.columns().iter().map(|n|n.as_str()).collect::<Vec<_>>(),"unique":value.unique(),"where":value.where_().map(|rows|predicates(rows))})
}
pub(crate) fn column(value: &DerivedColumn) -> Value {
    json!({"name":value.name().as_str(),"type":value.storage_type(),"nullable":value.nullable(),"origin":value.origin().key(),"generated":value.generated_kind().map(|k|k.key()),"default":value.default().map(default)})
}
pub(crate) fn profile(value: &StorageEngineAttachment) -> Result<Value, DiagnosticSet> {
    serde_json::from_str(&value.canonical_bytes()?).map_err(|_| {
        diagnostic::rule_invalid(diagnostic::MAPPING_INVALID, "profile-undecodable", None)
    })
}
pub(crate) fn table(attachment: &StorageProjectionAttachment, table: &DerivedTable) -> Value {
    let columns = table
        .columns()
        .iter()
        .map(|c| {
            let mut value = column(c);
            if c.origin().key() == "field" {
                if let Some(field) = attachment.entity(table.entity()).and_then(|e| {
                    e.fields()
                        .iter()
                        .find(|f| f.name().as_str() == c.name().as_str())
                }) {
                    value["sourceType"] = domain_type(field.field_type());
                }
            }
            value
        })
        .collect::<Vec<_>>();
    let checks = attachment
        .projections()
        .iter()
        .find(|p| p.namespace().key() == "postgres")
        .and_then(|p| p.tables().iter().find(|t| t.entity() == table.entity()))
        .map(|t| {
            t.checks()
                .iter()
                .map(|c| json!({"name":c.name().as_str(),"predicates":predicates(c.predicates())}))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({"table":table.table().as_str(),"entity":table.entity().as_str(),"columns":columns,"primaryKey":table.primary_key().iter().map(|n|n.as_str()).collect::<Vec<_>>(),"checks":checks,"enumMembers":super::postgres::ddl::declared_enum_members(attachment,table),"indexes":table.indexes().iter().map(index).collect::<Vec<_>>(),"foreignKeys":table.foreign_keys().iter().map(|f|json!({"column":f.column().as_str(),"referencesTable":f.references_table().as_str(),"onDelete":f.on_delete().key()})).collect::<Vec<_>>()})
}
pub(crate) fn join(value: &DerivedJoin) -> Value {
    json!({"table":value.table().as_str(),"columns":value.columns().iter().map(column).collect::<Vec<_>>(),"uniquePair":value.unique_pair(),"onOwnerDelete":value.on_owner_delete().key(),"onTargetDelete":value.on_target_delete().key()})
}
pub(crate) fn projection(
    attachment: &StorageProjectionAttachment,
    projection: &DerivedProjection,
) -> Value {
    let joins = projection
        .joins()
        .iter()
        .map(|j| {
            let mut value = join(j);
            value["references"] = json!([true, false]
                .iter()
                .map(|owner| super::postgres::ddl::referenced_join_target(
                    attachment, projection, j, *owner
                )
                .map(|(t, k)| json!({"table":t.as_str(),"column":k.as_str()})))
                .collect::<Vec<_>>());
            value
        })
        .collect::<Vec<_>>();
    json!({"tables":projection.tables().iter().map(|t|table(attachment,t)).collect::<Vec<_>>(),"joins":joins})
}
