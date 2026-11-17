//! Legacy type API delegates to the installed provider.
use super::super::{provider, Policies};
#[cfg(test)]
use super::super::{ArrayPolicy, JsonPolicy};
use crate::diagnostics::DiagnosticSet;
use crate::storage_projection::entity::DomainType;
/// Render one declared type through the external provider.
pub fn map_type(field_type: &DomainType, policies: &Policies) -> Result<String, DiagnosticSet> {
    provider::text(
        "storage.type",
        serde_json::json!({"type":provider::domain_type(field_type),"policies":provider::policies(policies)}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;

    fn policies(array: ArrayPolicy, local: bool, json: JsonPolicy) -> Policies {
        Policies {
            identifier_quote_always: true,
            json,
            enum_policy: crate::storage_engine::EnumPolicy::Check,
            array,
            time: crate::storage_engine::TimePolicy {
                instant_timestamptz: true,
                local_allowed: local,
            },
            pagination: crate::storage_engine::PaginationPolicy {
                offset_allowed: true,
                cursor_keyset: true,
            },
        }
    }

    #[test]
    fn the_scalar_table_matches_the_published_projection_table() {
        let policies = policies(ArrayPolicy::Native, false, JsonPolicy::Jsonb);
        assert_eq!(
            map_type(&DomainType::Boolean, &policies).unwrap(),
            "boolean"
        );
        assert_eq!(map_type(&DomainType::Integer, &policies).unwrap(), "bigint");
        assert_eq!(
            map_type(
                &DomainType::Decimal {
                    precision: 10,
                    scale: 2
                },
                &policies
            )
            .unwrap(),
            "numeric(10,2)"
        );
        assert_eq!(
            map_type(&DomainType::String { length: 64 }, &policies).unwrap(),
            "varchar(64)"
        );
        assert_eq!(map_type(&DomainType::Binary, &policies).unwrap(), "bytea");
    }

    #[test]
    fn json_array_policies_gate_the_rendering() {
        let strict = policies(ArrayPolicy::Native, false, JsonPolicy::Jsonb);
        assert_eq!(map_type(&DomainType::Json, &strict).unwrap(), "jsonb");
        let textual = policies(ArrayPolicy::Native, false, JsonPolicy::Json);
        assert_eq!(map_type(&DomainType::Json, &textual).unwrap(), "json");
        let native = policies(ArrayPolicy::Native, false, JsonPolicy::Jsonb);
        assert_eq!(
            map_type(
                &DomainType::Array {
                    element: Box::new(DomainType::Uuid),
                    max_items: None,
                },
                &native
            )
            .unwrap(),
            "uuid[]"
        );
        let json_arrays = policies(ArrayPolicy::Json, false, JsonPolicy::Jsonb);
        assert_eq!(
            map_type(
                &DomainType::Array {
                    element: Box::new(DomainType::Uuid),
                    max_items: None,
                },
                &json_arrays
            )
            .unwrap(),
            "jsonb"
        );
        let refused = policies(ArrayPolicy::Unsupported, false, JsonPolicy::Jsonb);
        let error = map_type(
            &DomainType::Array {
                element: Box::new(DomainType::Uuid),
                max_items: None,
            },
            &refused,
        )
        .expect_err("refused");
        assert_eq!(
            error.reason_ids().first().copied(),
            Some("storage-engine.render-unsupported")
        );
        // Instants always render as the timezoned type; the naive
        // timestamp is unrepresentable by the v1 domain vocabulary.
        assert_eq!(
            map_type(&DomainType::Timestamp, &strict).unwrap(),
            "timestamptz"
        );
        let naive = policies(ArrayPolicy::Native, true, JsonPolicy::Jsonb);
        assert_eq!(
            map_type(&DomainType::Timestamp, &naive).unwrap(),
            "timestamptz"
        );
    }

    #[test]
    fn enums_render_through_the_bounded_varchar_and_refuse_native() {
        let check = policies(ArrayPolicy::Native, false, JsonPolicy::Jsonb);
        assert_eq!(
            map_type(
                &DomainType::Enum {
                    members: vec!["red".to_owned(), "green".to_owned()]
                },
                &check
            )
            .unwrap(),
            "varchar(64)"
        );
        // The native_enum policy refuses explicitly: the 0.4.0 renderer
        // creates no enum types, and silence would invent a varchar.
        let mut native = policies(ArrayPolicy::Native, false, JsonPolicy::Jsonb);
        native.enum_policy = crate::storage_engine::EnumPolicy::NativeEnum;
        let error = map_type(
            &DomainType::Enum {
                members: vec!["red".to_owned()],
            },
            &native,
        )
        .expect_err("native_enum refuses");
        assert_eq!(
            error.reason_ids().first().copied(),
            Some("storage-engine.render-unsupported")
        );
    }
}
