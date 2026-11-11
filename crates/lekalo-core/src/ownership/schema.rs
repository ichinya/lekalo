//! One offline validator per closed family; parser and published schema agree
//! on required nullable fields and collection bounds, not just Rust field types.
use super::*;
use std::{collections::BTreeMap, sync::LazyLock};

static VALIDATORS: LazyLock<BTreeMap<String, jsonschema::Validator>> = LazyLock::new(|| {
    let schemas: &[(&str, &[u8])] = &[
        (
            "ownership-status",
            include_bytes!("../../../../contracts/ownership-status.schema.v1.0.0.json"),
        ),
        (
            "ownership-trace",
            include_bytes!("../../../../contracts/ownership-trace.schema.v1.0.0.json"),
        ),
        (
            "ownership-transition-request",
            include_bytes!("../../../../contracts/ownership-transition-request.schema.v1.0.0.json"),
        ),
        (
            "ownership-transition-plan",
            include_bytes!("../../../../contracts/ownership-transition-plan.schema.v1.0.0.json"),
        ),
        (
            "ownership-verification",
            include_bytes!("../../../../contracts/ownership-verification.schema.v1.0.0.json"),
        ),
        (
            "ownership-transition-receipt",
            include_bytes!("../../../../contracts/ownership-transition-receipt.schema.v1.0.0.json"),
        ),
        (
            "ownership-state",
            include_bytes!("../../../../contracts/ownership-state.schema.v1.0.0.json"),
        ),
        (
            "ownership-journal",
            include_bytes!("../../../../contracts/ownership-journal.schema.v1.0.0.json"),
        ),
        (
            "ownership-rollback-plan",
            include_bytes!("../../../../contracts/ownership-rollback-plan.schema.v1.0.0.json"),
        ),
    ];
    schemas
        .iter()
        .map(|(name, bytes)| {
            let schema: serde_json::Value =
                serde_json::from_slice(bytes).expect("embedded ownership schema");
            (
                format!("lekalo/{name}/v{VERSION}"),
                jsonschema::validator_for(&schema).expect("offline ownership validator"),
            )
        })
        .collect()
});

pub(crate) fn validate(value: &serde_json::Value) -> Outcome<()> {
    if let Some(version) = value.get("schemaVersion") {
        let version = version
            .as_str()
            .ok_or_else(|| fail("ownership-contract-version"))?;
        let validator = VALIDATORS
            .get(version)
            .ok_or_else(|| fail("ownership-contract-version"))?;
        if !validator.is_valid(value) {
            return Err(fail("ownership-schema"));
        }
    }
    Ok(())
}
