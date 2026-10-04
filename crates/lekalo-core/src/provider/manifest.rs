//! The typed, deterministic `lekalo provider describe` manifest
//! (issue #34): the AIFHub-consumable discovery handshake.
//!
//! The manifest is a closed object serialized with sorted keys and no
//! whitespace; its integrity is the `manifestDigest` over exactly that
//! canonical form with the digest field itself removed. It carries no
//! host identity, timestamps, or paths; the same compiled binary
//! projects byte-identical bytes on every host.
#![allow(non_snake_case)] // wire field names are the published contract

use serde::Serialize;
use serde_json::Value as Json;

use crate::provider::operations::{EffectClass, Operation, OPERATIONS};
use crate::provider::version::{
    DISCOVERY_COMMAND, IDENTITY, PRODUCT_VERSION, RECOMMENDED_BUDGET_TOKENS, SCHEMA_VERSION,
};

/// One pinned upstream contract identity, advertised so a consumer can
/// negotiate exact versions instead of inferring from the product
/// number.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ContractPin {
    /// The exact discriminator on the wire (`lekalo/.../v<version>`).
    schemaVersion: &'static str,
    /// The exact `dev.lekalo.*` family identity when the family
    /// publishes one; absent for families without an identity constant.
    #[serde(skip_serializing_if = "Option::is_none")]
    identity: Option<&'static str>,
}

impl ContractPin {
    /// The exact discriminator.
    pub const fn schema_version(&self) -> &'static str {
        self.schemaVersion
    }

    /// The family identity when published.
    pub const fn identity(&self) -> Option<&'static str> {
        self.identity
    }
}

/// The recommended workflow bounds. The core hard bounds stay the
/// owning contract families' constants; these are the smaller workflow
/// recommendations a consumer should default to.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Bounds {
    /// The recommended `lekalo context` budget inside a workflow run.
    recommendedContextBudgetTokens: u64,
    /// The hard core budget bound (`context.MAX_BUDGET_TOKENS`).
    maxContextBudgetTokens: u64,
    /// The exact maximum canonical impact/trace export bytes.
    maxExportBytes: usize,
}

impl Bounds {
    /// The recommended context budget.
    pub const fn recommended_context_budget_tokens(&self) -> u64 {
        self.recommendedContextBudgetTokens
    }

    /// The hard context budget bound.
    pub const fn max_context_budget_tokens(&self) -> u64 {
        self.maxContextBudgetTokens
    }

    /// The maximum export bytes.
    pub const fn max_export_bytes(&self) -> usize {
        self.maxExportBytes
    }
}

/// The full discovery manifest.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProviderManifest {
    /// The exact wire discriminator of this contract.
    schemaVersion: &'static str,
    /// The exact contract identity.
    identity: &'static str,
    /// The exact producing contract's product generation (`0.6.4`, the
    /// implementation commit's product version; may trail a later
    /// binary's `--version`).
    productVersion: &'static str,
    /// The discovery command (presentation form).
    discoveryCommand: &'static str,
    /// The exact target-protocol identity: a supported CLI operation
    /// never implies a configured target; generation additionally
    /// requires an installed adapter program.
    targetProtocolIdentity: &'static str,
    /// The pinned upstream output contract identities.
    schemaPins: Vec<ContractPin>,
    /// The advertised operations in canonical order.
    operations: Vec<OperationRef>,
    /// The published workflow bounds.
    bounds: Bounds,
    /// The canonical-JSON self-digest (`sha256:<hex>`); computed last
    /// over the manifest without this field.
    manifestDigest: String,
}

/// A compact operation reference inside the manifest: the identity and
/// effect of one advertised operation. The full option bounds stay in
/// [`docs/provider-contract.md`]; the closed wire keeps the negotiation
/// facts a consumer must pin.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OperationRef {
    id: &'static str,
    effect: EffectClass,
    outputSchema: &'static str,
    requiresProject: bool,
    requiresAdapter: bool,
}

impl OperationRef {
    /// The stable operation id.
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// The effect class.
    pub const fn effect(&self) -> EffectClass {
        self.effect
    }

    /// The output schema identity.
    pub const fn output_schema(&self) -> &'static str {
        self.outputSchema
    }

    /// Whether a validated project selection is required.
    pub const fn requires_project(&self) -> bool {
        self.requiresProject
    }

    /// Whether an explicit target adapter program is required.
    pub const fn requires_adapter(&self) -> bool {
        self.requiresAdapter
    }
}

impl From<&Operation> for OperationRef {
    fn from(operation: &Operation) -> Self {
        Self {
            id: operation.id(),
            effect: operation.effect(),
            outputSchema: operation.output_schema(),
            requiresProject: operation.requires_project(),
            requiresAdapter: operation.requires_adapter(),
        }
    }
}

impl ProviderManifest {
    /// Build the manifest from the compiled contract constants. Pure
    /// and deterministic: no filesystem, no environment, no clock.
    pub fn describe() -> Self {
        let pins = vec![
            ContractPin {
                schemaVersion: crate::context::version::SCHEMA_VERSION,
                identity: Some(crate::context::version::IDENTITY),
            },
            ContractPin {
                schemaVersion: crate::diagnostics::version::SCHEMA_VERSION,
                identity: None,
            },
            ContractPin {
                schemaVersion: crate::doctor::version::SCHEMA_VERSION,
                identity: Some(crate::doctor::version::IDENTITY),
            },
            ContractPin {
                schemaVersion: crate::provider::version::GENERATE_CHECK_SCHEMA,
                identity: None,
            },
            ContractPin {
                schemaVersion: crate::impact::SCHEMA_VERSION,
                identity: Some(crate::impact::IDENTITY),
            },
            ContractPin {
                schemaVersion: crate::orchestration::SCHEMA_VERSION,
                identity: Some(crate::orchestration::IDENTITY),
            },
            ContractPin {
                schemaVersion: crate::trace::assessment::REPORT_SCHEMA,
                identity: Some(crate::trace::assessment::REPORT_IDENTITY),
            },
            ContractPin {
                schemaVersion: crate::trace::version::SCHEMA_VERSION,
                identity: Some(crate::trace::version::IDENTITY),
            },
            ContractPin {
                schemaVersion: crate::trace::assessment::EVIDENCE_SCHEMA,
                identity: Some(crate::trace::assessment::EVIDENCE_IDENTITY),
            },
            ContractPin {
                schemaVersion: crate::validator::profile::PROFILE_SCHEMA_VERSION,
                identity: Some(crate::validator::profile::PROFILE_IDENTITY),
            },
            ContractPin {
                schemaVersion: crate::provider::version::VALIDATION_REPORT_SCHEMA,
                identity: None,
            },
        ];
        let operations = OPERATIONS.iter().map(OperationRef::from).collect();
        let mut manifest = Self {
            schemaVersion: SCHEMA_VERSION,
            identity: IDENTITY,
            productVersion: PRODUCT_VERSION,
            discoveryCommand: DISCOVERY_COMMAND,
            targetProtocolIdentity: crate::target_protocol::version::IDENTITY,
            schemaPins: pins,
            operations,
            bounds: Bounds {
                recommendedContextBudgetTokens: RECOMMENDED_BUDGET_TOKENS,
                maxContextBudgetTokens: crate::context::version::MAX_BUDGET_TOKENS,
                maxExportBytes: crate::impact::MAX_EXPORT_BYTES,
            },
            manifestDigest: String::new(),
        };
        manifest.manifestDigest = format!("sha256:{}", manifest.self_digest_hex());
        manifest
    }

    /// The exact wire discriminator.
    pub const fn schema_version(&self) -> &'static str {
        self.schemaVersion
    }

    /// The exact contract identity.
    pub const fn identity(&self) -> &'static str {
        self.identity
    }

    /// The exact product version of the producing binary.
    pub const fn product_version(&self) -> &'static str {
        self.productVersion
    }

    /// The advertised operations.
    pub fn operations(&self) -> &[OperationRef] {
        &self.operations
    }

    /// The `sha256:<hex>` self-digest over the canonical JSON of the
    /// manifest without the `manifestDigest` field.
    pub fn manifest_digest(&self) -> &str {
        &self.manifestDigest
    }

    /// Canonical JSON: sorted keys, no whitespace, exact scalar
    /// spellings. Rebuilt from the serialized document so the digest
    /// domain is the emitted bytes' semantics, not the struct order.
    fn canonical_json_without_digest(&self) -> String {
        let mut value: Json = serde_json::to_value(self).expect("manifest serializes");
        let object = value.as_object_mut().expect("manifest is an object");
        object.remove("manifestDigest");
        canonicalize(&value)
    }

    fn self_digest_hex(&self) -> String {
        crate::digest::sha256_hex(self.canonical_json_without_digest().as_bytes())
    }

    /// The exact JSON envelope bytes of `lekalo provider describe --json`:
    /// the `status: valid` receipt with the manifest in field order.
    pub fn to_receipt_json(&self) -> String {
        let manifest_json = serde_json::to_string_pretty(self).expect("manifest serializes");
        format!("{{\n  \"status\": \"valid\",\n  \"manifest\": {manifest_json}\n}}")
    }

    /// The stable one-line human summary.
    pub fn to_human_summary(&self) -> String {
        let ids: Vec<&str> = self.operations.iter().map(|o| o.id).collect();
        format!(
            "provider {} product {} operations {}",
            self.identity,
            self.productVersion,
            ids.join(",")
        )
    }

    /// Verify the recorded self-digest against a recomputation. The
    /// conformance round-trip the consumer may repeat.
    pub fn verify_digest(&self) -> bool {
        let expected = format!("sha256:{}", self.self_digest_hex());
        expected == self.manifestDigest
    }
}

/// Deterministic canonical JSON: object keys sorted lexicographically
/// (byte order), no whitespace, numbers through `serde_json`'s exact
/// integer rendering, strings escaped by `serde_json`.
fn canonicalize(value: &Json) -> String {
    match value {
        Json::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let body: Vec<String> = keys
                .into_iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).expect("key serializes"),
                        canonicalize(&map[key])
                    )
                })
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Json::Array(items) => {
            let body: Vec<String> = items.iter().map(canonicalize).collect();
            format!("[{}]", body.join(","))
        }
        Json::String(text) => serde_json::to_string(text).expect("string serializes"),
        Json::Bool(flag) => flag.to_string(),
        Json::Null => "null".to_owned(),
        Json::Number(number) => number.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_deterministic_across_instances() {
        let first = ProviderManifest::describe();
        let second = ProviderManifest::describe();
        assert_eq!(first.to_receipt_json(), second.to_receipt_json());
    }

    #[test]
    fn manifest_carries_no_host_identity() {
        let rendered = ProviderManifest::describe().to_receipt_json();
        assert!(!rendered.contains("C:\\"), "no Windows path");
        assert!(!rendered.contains('\\'), "no backslash");
        assert!(
            !rendered.to_ascii_lowercase().contains("timestamp"),
            "no timestamps"
        );
    }

    #[test]
    fn digest_round_trips_and_binds_the_payload() {
        let manifest = ProviderManifest::describe();
        assert!(manifest.verify_digest());
        assert!(manifest.manifest_digest().starts_with("sha256:"));
        // The digest domain excludes the digest field itself.
        let without = manifest.canonical_json_without_digest();
        assert!(!without.contains("manifestDigest"));
    }

    #[test]
    fn receipt_json_is_the_status_first_envelope() {
        let json = ProviderManifest::describe().to_receipt_json();
        let value: Json = serde_json::from_str(&json).expect("envelope parses");
        assert_eq!(value["status"], "valid");
        assert_eq!(value["manifest"]["schemaVersion"], SCHEMA_VERSION);
        assert_eq!(value["manifest"]["identity"], IDENTITY);
        assert_eq!(
            value["manifest"]["productVersion"],
            crate::provider::version::PRODUCT_VERSION
        );
        assert_eq!(
            value["manifest"]["targetProtocolIdentity"],
            crate::target_protocol::version::IDENTITY
        );
    }

    #[test]
    fn pins_cover_the_ten_output_families_and_evidence_input() {
        let manifest = ProviderManifest::describe();
        let schemas: Vec<_> = manifest
            .schemaPins
            .iter()
            .map(|pin| pin.schema_version())
            .collect();
        assert_eq!(
            schemas,
            vec![
                "lekalo/context/v0.2.16",
                "lekalo/diagnostic/v0.2.16",
                "lekalo/doctor/v0.3.2",
                "lekalo/generate-check/v0.6.3",
                "lekalo/impact/v0.2.16",
                "lekalo/orchestration/v0.2.16",
                "lekalo/trace-assessment/v0.6.4",
                "lekalo/trace-manifest/v0.2.16",
                "lekalo/trace-validation-evidence/v0.6.4",
                "lekalo/validation-profile/v0.6.4",
                "lekalo/validation-report/v0.6.4",
            ]
        );
    }

    #[test]
    fn bounds_are_within_the_core_hard_bounds() {
        let manifest = ProviderManifest::describe();
        assert_eq!(
            manifest.bounds.max_context_budget_tokens(),
            crate::context::version::MAX_BUDGET_TOKENS
        );
        assert_eq!(
            manifest.bounds.max_export_bytes(),
            crate::impact::MAX_EXPORT_BYTES
        );
        assert!(manifest.bounds.recommended_context_budget_tokens() < 1_000_000);
    }

    #[test]
    fn canonicalize_sorts_keys_and_removes_whitespace() {
        let value: Json = serde_json::from_str("{\"b\":1,\"a\":{\"d\":[true,null],\"c\":\"x\"}}")
            .expect("parses");
        assert_eq!(
            canonicalize(&value),
            r#"{"a":{"c":"x","d":[true,null]},"b":1}"#
        );
    }
}
