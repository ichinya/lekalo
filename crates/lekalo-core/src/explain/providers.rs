//! Offline provider explanation references, separate from frozen diagnostics.
//! Catalogs supply pointers only: importing one never executes an adapter,
//! fetches a URL, changes core rule semantics, or authorizes a recipe.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::diagnostics::{OriginalCode, ProviderNamespace};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProviderReference {
    pub namespace: String,
    pub original_code: String,
    /// The exact provider-owned pointer, or null when no offline entry exists.
    pub explanation_ref: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Wire {
    schema_version: String,
    identity: String,
    entries: Vec<ProviderReference>,
}

#[derive(Default)]
pub struct ProviderCatalog(BTreeMap<(String, String), String>);

impl ProviderCatalog {
    pub fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > super::MAX_REPORT_BYTES {
            return Err("provider-catalog-limit");
        }
        let value =
            crate::adoption::wire::unique_json(bytes).map_err(|_| "provider-catalog-json")?;
        let wire: Wire = serde_json::from_value(value).map_err(|_| "provider-catalog-shape")?;
        if wire.schema_version != "lekalo/explain-provider-catalog/v0.8.0"
            || wire.identity != "dev.lekalo.explain-provider-catalog@0.8.0"
            || wire.entries.len() > 512
        {
            return Err("provider-catalog-version");
        }
        let mut entries = BTreeMap::new();
        for entry in wire.entries {
            let Some(reference) = entry.explanation_ref else {
                return Err("provider-reference-missing");
            };
            if ProviderNamespace::new(entry.namespace.clone()).is_none()
                || OriginalCode::new(entry.original_code.clone()).is_none()
                || reference.is_empty()
                || reference.chars().count() > 512
                || reference.chars().any(char::is_control)
            {
                return Err("provider-reference-invalid");
            }
            if entries
                .insert((entry.namespace, entry.original_code), reference)
                .is_some()
            {
                return Err("provider-reference-duplicate");
            }
        }
        Ok(Self(entries))
    }

    pub fn references(&self, item: &Value) -> Result<Vec<ProviderReference>, &'static str> {
        let Some(metadata) = item.get("metadata") else {
            return Ok(Vec::new());
        };
        let map = metadata.as_object().ok_or("provider-metadata-shape")?;
        if map.len() > 8 {
            return Err("provider-metadata-limit");
        }
        let mut references = Vec::new();
        for (namespace, metadata) in map {
            let metadata = metadata.as_object().ok_or("provider-metadata-shape")?;
            let code = metadata
                .get("original_code")
                .and_then(Value::as_str)
                .ok_or("provider-code-missing")?;
            if metadata.len() != 1
                || ProviderNamespace::new(namespace.clone()).is_none()
                || OriginalCode::new(code.to_owned()).is_none()
            {
                return Err("provider-metadata-invalid");
            }
            references.push(ProviderReference {
                namespace: namespace.clone(),
                original_code: code.to_owned(),
                explanation_ref: self.0.get(&(namespace.clone(), code.to_owned())).cloned(),
            });
        }
        Ok(references)
    }
}
