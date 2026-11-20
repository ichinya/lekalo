//! Admission of data-only binding rules from a pinned evidence package.
use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value;

use crate::target_protocol::{scan_evidence::Fact, scopes};

use super::types::SymbolKind;

const VERSION: &str = "lekalo/evidence-binding-rules/v1.3.0";
const INVALID: &str = "evidence-binding-rule-invalid";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Rule {
    schema: String,
    kind: SymbolKind,
    identity: Vec<String>,
    #[serde(rename = "match")]
    fields: BTreeMap<String, String>,
    source_roles: Vec<String>,
}

impl Rule {
    pub(super) fn parse(bytes: &[u8], payload_schema: &Value) -> Result<Self, &'static str> {
        if bytes
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace())
            != Some(b'{')
        {
            return Err(INVALID);
        }
        let value = crate::runtime::config::value(bytes).map_err(|_| INVALID)?;
        let rule: Self = serde_json::from_value(value).map_err(|_| INVALID)?;
        let allowed_attributes = match rule.kind {
            SymbolKind::Endpoint => &["id", "method", "path"][..],
            _ => &["id"][..],
        };
        if rule.schema != VERSION
            || !bounded_unique(&rule.identity)
            || rule.fields.is_empty()
            || rule
                .fields
                .keys()
                .any(|field| !allowed_attributes.contains(&field.as_str()))
            || !bounded_unique(&rule.source_roles)
            || !rule.source_roles.iter().any(|role| role == "subject")
            || rule.source_roles.iter().any(|role| !scopes::is_token(role))
        {
            return Err(INVALID);
        }

        if payload_schema["type"] != "object" {
            return Err("evidence-binding-payload-field");
        }
        let required = payload_schema["required"]
            .as_array()
            .ok_or("evidence-binding-payload-field")?;
        for field in rule.identity.iter().chain(rule.fields.values()) {
            if !scopes::is_token(field)
                || !required
                    .iter()
                    .any(|candidate| candidate.as_str() == Some(field))
                || !matches!(
                    payload_schema["properties"][field]["type"].as_str(),
                    Some("string" | "number" | "integer" | "boolean")
                )
            {
                return Err("evidence-binding-payload-field");
            }
        }
        Ok(rule)
    }

    pub(super) fn verify_fact(&self, fact: &Fact) -> Result<(), &'static str> {
        for field in self.identity.iter().chain(self.fields.values()) {
            if !matches!(
                fact.payload.get(field),
                Some(Value::String(_) | Value::Number(_) | Value::Bool(_))
            ) {
                return Err("evidence-binding-payload-field");
            }
        }
        if self
            .source_roles
            .iter()
            .any(|role| role != "subject" && !fact.references.contains_key(role))
        {
            return Err("evidence-binding-source-role");
        }
        Ok(())
    }
}

fn bounded_unique(values: &[String]) -> bool {
    (1..=8).contains(&values.len()) && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}
