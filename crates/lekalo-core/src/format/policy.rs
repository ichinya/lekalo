//! The closed built-in format policy (issue #94).
//!
//! The pinned `format-policy.v0.8.0` contract is the only style authority:
//! there is no user configuration. The embedded bytes are validated once
//! per process and their digest is verified against the pinned
//! `policyDigest` member so a drifted contract fails closed.

use serde::Deserialize;

/// One formatter limit from the policy contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    #[serde(rename = "maxFiles")]
    pub max_files: usize,
    #[serde(rename = "maxCandidateTotalBytes")]
    pub max_candidate_total_bytes: usize,
    #[serde(rename = "maxLossRows")]
    pub max_loss_rows: usize,
    #[serde(rename = "maxDocumentBytes")]
    pub max_document_bytes: usize,
}

/// The validated built-in policy.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(rename = "schema_version")]
    #[allow(dead_code)] // identity pin; validated structurally below
    schema_version: String,
    identity: String,
    #[serde(rename = "encoding")]
    #[allow(dead_code)] // pinned by the contract consts
    encoding: String,
    #[serde(rename = "lineEnding")]
    #[allow(dead_code)] // pinned by the contract consts
    line_ending: String,
    #[serde(rename = "finalNewline")]
    #[allow(dead_code)] // pinned by the contract consts
    final_newline: bool,
    #[serde(rename = "indent")]
    #[allow(dead_code)] // pinned by the contract consts
    indent: Indent,
    #[serde(rename = "quotes")]
    #[allow(dead_code)] // pinned by the contract consts
    quotes: serde_json::Value,
    #[serde(rename = "scalars")]
    #[allow(dead_code)] // pinned by the contract consts
    scalars: serde_json::Value,
    #[serde(rename = "collections")]
    #[allow(dead_code)] // pinned by the contract consts
    collections: serde_json::Value,
    #[serde(rename = "ordering")]
    #[allow(dead_code)] // pinned by the contract consts
    ordering: serde_json::Value,
    #[serde(rename = "trivia")]
    #[allow(dead_code)] // pinned by the contract consts
    trivia: serde_json::Value,
    #[serde(rename = "paths")]
    #[allow(dead_code)] // pinned by the contract consts
    paths: serde_json::Value,
    #[serde(rename = "digestRecipe")]
    #[allow(dead_code)] // pinned by the contract consts
    digest_recipe: serde_json::Value,
    pub limits: Limits,
    #[serde(rename = "modelVersion")]
    #[allow(dead_code)] // read through the contract consts
    pub model_version: String,
    #[serde(rename = "policyDigest")]
    pub digest: String,
}

impl Policy {
    /// The exact contract version.
    pub const VERSION: &'static str = "0.8.0";
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Indent {
    #[allow(dead_code)] // pinned by the contract consts
    style: String,
    #[allow(dead_code)] // pinned by the contract consts
    width: u32,
}

impl Policy {
    /// The exact wire discriminator of the policy contract.
    pub const SCHEMA_VERSION: &'static str = "lekalo/format-policy/v0.8.0";
    /// The exact policy identity.
    pub const IDENTITY: &'static str = "dev.lekalo.format-policy@0.8.0";

    /// Validate and pin the embedded policy once per process.
    pub fn embedded() -> Result<Self, ()> {
        static POLICY: std::sync::LazyLock<Result<Policy, ()>> =
            std::sync::LazyLock::new(|| Policy::from_bytes(crate::format::POLICY_BYTES));
        POLICY.clone()
    }

    /// Parse and validate policy bytes: identity pin, digest recompute,
    /// and the closed shape.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ()> {
        let policy: Policy = serde_json::from_slice(bytes).map_err(|_| ())?;
        if policy.schema_version != Self::SCHEMA_VERSION || policy.identity != Self::IDENTITY {
            return Err(());
        }
        let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| ())?;
        let digest = policy.digest.clone();
        if compute_policy_digest(&value)? != digest {
            return Err(());
        }
        Ok(policy)
    }
}

impl Clone for Policy {
    fn clone(&self) -> Self {
        Self::from_bytes(crate::format::POLICY_BYTES).expect("embedded policy is pinned")
    }
}

/// The canonical compact JSON of one value: byte-sorted object keys, no
/// whitespace — the same recipe the contract gate uses.
pub(crate) fn canonical_json(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Null => out.push_str("null"),
        serde_json::Value::Bool(true) => out.push_str("true"),
        serde_json::Value::Bool(false) => out.push_str("false"),
        serde_json::Value::Number(number) => out.push_str(&number.to_string()),
        serde_json::Value::String(text) => {
            crate::loader::canonical::write_json_string(text, out);
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                canonical_json(item, out);
            }
            out.push(']');
        }
        serde_json::Value::Object(entries) => {
            let mut keys: Vec<&String> = entries.keys().collect();
            keys.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
            out.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                crate::loader::canonical::write_json_string(key, out);
                out.push(':');
                canonical_json(&entries[*key], out);
            }
            out.push('}');
        }
    }
}

/// The policy digest recipe: SHA-256 over the canonical compact JSON of the
/// whole document excluding its own `policyDigest` member.
pub(crate) fn compute_policy_digest(value: &serde_json::Value) -> Result<String, ()> {
    let serde_json::Value::Object(entries) = value else {
        return Err(());
    };
    let mut reduced = entries.clone();
    reduced.remove("policyDigest");
    let mut canonical = String::new();
    canonical_json(&serde_json::Value::Object(reduced), &mut canonical);
    Ok(format!(
        "sha256:{}",
        crate::digest::sha256_hex(canonical.as_bytes())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_policy_pins_its_own_digest() {
        let policy = Policy::embedded().expect("embedded policy is valid");
        assert_eq!(Policy::VERSION, "0.8.0");
        assert_eq!(policy.model_version, "0.2.16");
        assert_eq!(policy.limits.max_files, 4096);
        assert!(policy.digest.starts_with("sha256:"));
    }

    #[test]
    fn a_drifted_digest_fails_closed() {
        let mut value: serde_json::Value =
            serde_json::from_slice(crate::format::POLICY_BYTES).expect("policy json");
        value["limits"]["maxFiles"] = serde_json::json!(1);
        let mut bytes = Vec::new();
        serde_json::to_writer(&mut bytes, &value).expect("serialize");
        assert!(Policy::from_bytes(&bytes).is_err());
    }

    #[test]
    fn canonical_json_sorts_keys_by_bytes() {
        let value = serde_json::json!({ "b": 1, "a": { "z": true, "a": null } });
        let mut out = String::new();
        canonical_json(&value, &mut out);
        assert_eq!(out, r#"{"a":{"a":null,"z":true},"b":1}"#);
    }
}
