//! The published PostgreSQL type table (issue #69): the exact
//! rendering of one domain value type under one profile's policies.
//!
//! The table extends the #65 namespace table with the 0.4.0 domain
//! arms and the policy gates: JSON defaults to `jsonb` with the
//! textual type as explicit opt-in, arrays render natively, as JSON,
//! or refuse explicitly, and the timezone-naive timestamp renders
//! only where the profile declares it allowed. Nothing is invented:
//! a rendering the profile does not declare refuses with a registered
//! diagnostic instead of guessing.

use crate::diagnostics::DiagnosticSet;
use crate::storage_projection::entity::DomainType;

use super::super::diagnostic::{self, RENDER_UNSUPPORTED};
use super::super::{ArrayPolicy, JsonPolicy, Policies};

/// Render one domain value type under the declared policies.
pub fn map_type(field_type: &DomainType, policies: &Policies) -> Result<String, DiagnosticSet> {
    let rendered = match field_type {
        DomainType::Boolean => "boolean".to_owned(),
        DomainType::Integer => "bigint".to_owned(),
        DomainType::Decimal { precision, scale } => format!("numeric({precision},{scale})"),
        DomainType::String { length } => format!("varchar({length})"),
        DomainType::Text => "text".to_owned(),
        DomainType::Uuid => "uuid".to_owned(),
        DomainType::Date => "date".to_owned(),
        DomainType::Timestamp => {
            // The v1 schema pins `time.instant` to the timezoned type;
            // the timezone-naive type is gateable policy surface that
            // no v1 domain type names.
            "timestamptz".to_owned()
        }
        DomainType::Binary => "bytea".to_owned(),
        DomainType::Json => match policies.json() {
            JsonPolicy::Jsonb => "jsonb".to_owned(),
            JsonPolicy::Json => "json".to_owned(),
        },
        DomainType::Enum { .. } => match policies.enum_policy() {
            // The bounded member CHECK over a varchar is the rendered
            // answer; the member list rides the derived column.
            crate::storage_engine::EnumPolicy::Check => "varchar(64)".to_owned(),
            // The 0.4.0 renderer creates no native enum types; the
            // declared policy refuses explicitly instead of silently
            // yielding a varchar.
            crate::storage_engine::EnumPolicy::NativeEnum => {
                return Err(diagnostic::rule_invalid(
                    RENDER_UNSUPPORTED,
                    "native-enum",
                    None,
                ));
            }
        },
        DomainType::Array { element, .. } => match policies.array() {
            ArrayPolicy::Native => format!("{}[]", map_type(element, policies)?),
            ArrayPolicy::Json => "jsonb".to_owned(),
            ArrayPolicy::Unsupported => {
                return Err(diagnostic::rule_invalid(
                    RENDER_UNSUPPORTED,
                    "array-unsupported",
                    None,
                ));
            }
        },
    };
    Ok(rendered)
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
                members: vec!["red".to_owned()]
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
