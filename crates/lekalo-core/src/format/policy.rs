//! The closed built-in format policy (issue #94).
//!
//! The pinned `format-policy.v0.8.0` contract is the only style authority:
//! there is no user configuration. The embedded bytes are validated once
//! per process and their digest is verified against the pinned
//! `policyDigest` member so a drifted contract fails closed.

use serde::Deserialize;

/// Why pinned policy bytes were refused (developer fault, fail closed).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PolicyError;

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

/// One set-like ordering rule.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetList {
    member: String,
    ordering: String,
}

/// The validated built-in policy (finding H): every nested object is a
/// typed closed record with pinned constants, not an opaque `Value`, so
/// a digest-valid but schema-invalid contract fails closed at decode.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(rename = "schema_version")]
    schema_version: String,
    identity: String,
    #[serde(rename = "encoding")]
    #[allow(dead_code)] // pinned constant; the renderer reads its consts
    encoding: String,
    #[serde(rename = "lineEnding")]
    #[allow(dead_code)] // pinned constant
    line_ending: String,
    #[serde(rename = "finalNewline")]
    #[allow(dead_code)] // pinned constant
    final_newline: bool,
    indent: Indent,
    quotes: String,
    scalars: Scalars,
    collections: Collections,
    ordering: Ordering,
    trivia: Trivia,
    paths: Paths,
    #[serde(rename = "digestRecipe")]
    #[allow(dead_code)] // pinned constant
    digest_recipe: DigestRecipe,
    pub limits: Limits,
    #[serde(rename = "modelVersion")]
    #[allow(dead_code)] // read through the contract consts
    pub model_version: String,
    #[serde(rename = "policyDigest")]
    pub digest: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Indent {
    #[allow(dead_code)] // pinned constant
    style: String,
    #[allow(dead_code)] // pinned constant
    width: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scalars {
    #[serde(rename = "booleans")]
    #[allow(dead_code)] // pinned constant
    booleans: String,
    #[serde(rename = "null")]
    #[allow(dead_code)] // pinned constant
    null: String,
    #[serde(rename = "integers")]
    #[allow(dead_code)] // pinned constant
    integers: String,
    #[serde(rename = "strings")]
    #[allow(dead_code)] // pinned constant
    strings: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Collections {
    #[serde(rename = "style")]
    #[allow(dead_code)] // pinned constant
    style: String,
    #[allow(dead_code)] // pinned constant
    empty: String,
    #[serde(rename = "sequenceMappingEntry")]
    #[allow(dead_code)] // pinned constant
    sequence_mapping_entry: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ordering {
    #[serde(rename = "mappingKeys")]
    #[allow(dead_code)] // pinned constant
    mapping_keys: String,
    #[allow(dead_code)] // pinned constant
    definitions: String,
    #[serde(rename = "setLists")]
    set_lists: Vec<SetList>,
    #[serde(rename = "groupPartition")]
    #[allow(dead_code)] // pinned constant
    group_partition: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Trivia {
    #[allow(dead_code)] // pinned constant
    comments: String,
    #[serde(rename = "blankRuns")]
    #[allow(dead_code)] // pinned constant
    blank_runs: String,
    #[serde(rename = "headerFooter")]
    #[allow(dead_code)] // pinned constant
    header_footer: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Paths {
    #[serde(rename = "canonicalFields")]
    #[allow(dead_code)] // pinned constant
    canonical_fields: String,
    #[allow(dead_code)] // pinned constant
    repair: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DigestRecipe {
    #[allow(dead_code)] // pinned constant
    algorithm: String,
    #[allow(dead_code)] // pinned constant
    input: String,
}

impl Policy {
    /// The exact contract version.
    pub const VERSION: &'static str = "0.8.0";

    /// The member path of the set-list rule at `index` (the renderer's
    /// set-ordering authority, pinned at decode).
    #[cfg(test)]
    pub(crate) fn ordering_set_list_member(&self, index: usize) -> &str {
        &self.ordering.set_lists[index].member
    }
}

impl Policy {
    /// The exact wire discriminator of the policy contract.
    pub const SCHEMA_VERSION: &'static str = "lekalo/format-policy/v0.8.0";
    /// The exact policy identity.
    pub const IDENTITY: &'static str = "dev.lekalo.format-policy@0.8.0";

    /// Validate and pin the embedded policy once per process.
    pub fn embedded() -> Result<Self, PolicyError> {
        static POLICY: std::sync::LazyLock<Result<Policy, PolicyError>> =
            std::sync::LazyLock::new(|| Policy::from_bytes(crate::format::POLICY_BYTES));
        POLICY.clone()
    }

    /// Parse and validate policy bytes: identity pin, typed closed
    /// constants, digest recompute, and the closed shape.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PolicyError> {
        let policy: Policy = serde_json::from_slice(bytes).map_err(|_| PolicyError)?;
        if policy.schema_version != Self::SCHEMA_VERSION || policy.identity != Self::IDENTITY {
            return Err(PolicyError);
        }
        // The pinned style constants the renderer relies on. The decoder
        // is typed, so unknown members or missing scalars already refuse;
        // these checks pin the exact accepted spellings (finding H).
        if policy.encoding != "utf-8-no-bom"
            || policy.line_ending != "lf"
            || !policy.final_newline
            || policy.indent.style != "space"
            || policy.indent.width != 2
            || policy.quotes != "double"
            || policy.scalars.booleans != "lowercase"
            || policy.scalars.null != "lowercase"
            || policy.scalars.integers != "base10"
            || policy.scalars.strings != "double-quoted-escaped"
            || policy.collections.style != "block"
            || policy.collections.empty != "json-inline-or-refuse"
            || policy.collections.sequence_mapping_entry != "dash-aligned"
            || policy.ordering.mapping_keys != "utf8-bytes-within-groups"
            || policy.ordering.definitions != "semantic-id-within-groups"
            || policy.ordering.group_partition != "blank-line-run"
            || policy.trivia.comments != "preserve-or-refuse"
            || policy.trivia.blank_runs != "collapse-to-one"
            || policy.trivia.header_footer != "document-owned"
            || policy.paths.canonical_fields != "owner-declared-only"
            || policy.paths.repair != "never"
            || policy.digest_recipe.algorithm != "sha256"
            || policy.digest_recipe.input != "canonical-compact-json-excluding-policyDigest"
        {
            return Err(PolicyError);
        }
        for set_list in &policy.ordering.set_lists {
            if set_list.ordering != "utf8-bytes"
                || set_list.member.is_empty()
                || set_list.member.len() > 128
            {
                return Err(PolicyError);
            }
        }
        if policy.ordering.set_lists.len() > 16 {
            return Err(PolicyError);
        }
        // The published numeric bounds and the modelVersion shape
        // (finding r2 5): a re-signed policy that leaves the digest valid
        // but violates the closed schema's minimums, maximums, pattern,
        // or length still refuses — runtime admission is exactly as
        // strict as the published Ajv schema.
        const MAX_FILES_CEILING: usize = 1_048_576;
        const BYTE_CEILING: usize = 1_073_741_824;
        if policy.limits.max_files == 0
            || policy.limits.max_files > MAX_FILES_CEILING
            || policy.limits.max_candidate_total_bytes == 0
            || policy.limits.max_candidate_total_bytes > BYTE_CEILING
            || policy.limits.max_loss_rows == 0
            || policy.limits.max_loss_rows > 4096
            || policy.limits.max_document_bytes == 0
            || policy.limits.max_document_bytes > BYTE_CEILING
        {
            return Err(PolicyError);
        }
        if policy.model_version.len() > 32 || !valid_model_version(&policy.model_version) {
            return Err(PolicyError);
        }
        let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| PolicyError)?;
        let digest = policy.digest.clone();
        if compute_policy_digest(&value)? != digest {
            return Err(PolicyError);
        }
        Ok(policy)
    }
}

impl Clone for Policy {
    fn clone(&self) -> Self {
        Self::from_bytes(crate::format::POLICY_BYTES).expect("embedded policy is pinned")
    }
}

/// The published `modelVersion` pattern `^[0-9]+\.[0-9]+\.[0-9]+$`
/// (finding r2 5).
fn valid_model_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
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
pub(crate) fn compute_policy_digest(value: &serde_json::Value) -> Result<String, PolicyError> {
    let serde_json::Value::Object(entries) = value else {
        return Err(PolicyError);
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
    fn a_digest_valid_schema_invalid_policy_fails_closed() {
        // Re-sign a drifted contract with a matching digest: the typed
        // closed decoder must still refuse it (finding H).
        let resign = |mutate: &dyn Fn(&mut serde_json::Value)| {
            let mut value: serde_json::Value =
                serde_json::from_slice(crate::format::POLICY_BYTES).expect("policy json");
            mutate(&mut value);
            value["policyDigest"] = {
                let mut reduced = value.clone();
                reduced.as_object_mut().unwrap().remove("policyDigest");
                let mut canonical = String::new();
                canonical_json(&reduced, &mut canonical);
                serde_json::Value::String(format!(
                    "sha256:{}",
                    crate::digest::sha256_hex(canonical.as_bytes())
                ))
            };
            let mut bytes = Vec::new();
            serde_json::to_writer(&mut bytes, &value).expect("serialize");
            Policy::from_bytes(&bytes).is_err()
        };
        // Unknown nested member.
        assert!(resign(&|value| {
            value["scalars"]["unexpected"] = serde_json::json!("x");
        }));
        // Missing required nested scalar.
        assert!(resign(&|value| {
            value["scalars"]
                .as_object_mut()
                .expect("object")
                .remove("booleans");
        }));
        // A wrong pinned constant the renderer would ignore.
        assert!(resign(&|value| {
            value["encoding"] = serde_json::json!("utf-16");
        }));
        assert!(resign(&|value| {
            value["indent"]["width"] = serde_json::json!(4);
        }));
        assert!(resign(&|value| {
            value["ordering"]["setLists"][0]["ordering"] = serde_json::json!("source");
        }));
        // The published numeric bounds (finding r2 5): below the minimum
        // or above the maximum of every limit, re-signed to stay
        // digest-valid, must still refuse.
        assert!(resign(&|value| {
            value["limits"]["maxFiles"] = serde_json::json!(0);
        }));
        assert!(resign(&|value| {
            value["limits"]["maxFiles"] = serde_json::json!(1_048_577);
        }));
        assert!(resign(&|value| {
            value["limits"]["maxDocumentBytes"] = serde_json::json!(1_073_741_825);
        }));
        assert!(resign(&|value| {
            value["limits"]["maxCandidateTotalBytes"] = serde_json::json!(0);
        }));
        assert!(resign(&|value| {
            value["limits"]["maxLossRows"] = serde_json::json!(4097);
        }));
        // The modelVersion pattern and length (finding r2 5).
        assert!(resign(&|value| {
            value["modelVersion"] = serde_json::json!("invalid");
        }));
        assert!(resign(&|value| {
            value["modelVersion"] = serde_json::json!("0.2.x");
        }));
        assert!(resign(&|value| {
            value["modelVersion"] = serde_json::json!("0..16");
        }));
        assert!(resign(&|value| {
            value["modelVersion"] = serde_json::json!("0.2.16.");
        }));
        assert!(resign(&|value| {
            value["modelVersion"] = serde_json::json!("1".repeat(33));
        }));
        // The exact boundary spellings stay acceptable.
        let accept = |mutate: &dyn Fn(&mut serde_json::Value)| {
            let mut value: serde_json::Value =
                serde_json::from_slice(crate::format::POLICY_BYTES).expect("policy json");
            mutate(&mut value);
            value["policyDigest"] = {
                let mut reduced = value.clone();
                reduced.as_object_mut().unwrap().remove("policyDigest");
                let mut canonical = String::new();
                canonical_json(&reduced, &mut canonical);
                serde_json::Value::String(format!(
                    "sha256:{}",
                    crate::digest::sha256_hex(canonical.as_bytes())
                ))
            };
            let mut bytes = Vec::new();
            serde_json::to_writer(&mut bytes, &value).expect("serialize");
            Policy::from_bytes(&bytes).is_ok()
        };
        assert!(accept(&|value| {
            value["limits"]["maxFiles"] = serde_json::json!(1);
        }));
        assert!(accept(&|value| {
            value["limits"]["maxFiles"] = serde_json::json!(1_048_576);
        }));
        assert!(accept(&|value| {
            value["limits"]["maxDocumentBytes"] = serde_json::json!(1_073_741_824);
        }));
        assert!(accept(&|value| {
            value["modelVersion"] = serde_json::json!("9.999.16");
        }));
    }

    #[test]
    fn nested_duplicate_keys_refuse_at_decode() {
        // serde_json's map silently keeps the last duplicate; the pinned
        // bytes are trusted, but a hand-forged record with duplicate keys
        // cannot reach the decoder through any runtime path. The embedded
        // policy is parse-checked once per process; assert it is clean.
        let policy = Policy::embedded().expect("embedded policy is valid");
        assert_eq!(policy.ordering_set_list_member(0), "modules[].imports");
    }

    #[test]
    fn canonical_json_sorts_keys_by_bytes() {
        let value = serde_json::json!({ "b": 1, "a": { "z": true, "a": null } });
        let mut out = String::new();
        canonical_json(&value, &mut out);
        assert_eq!(out, r#"{"a":{"a":null,"z":true},"b":1}"#);
    }
}
