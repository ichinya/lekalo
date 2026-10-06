//! Closed wire contracts for the issue #113 module API family.
//!
//! Every family pins its exact identity and discriminator at the 0.8.0
//! product generation; input snapshots, lock payloads and comparison
//! receipts are distinct digest domains over canonical bytes (compact
//! UTF-8 JSON with byte-sorted object keys). Closed structures reject
//! unknown fields, duplicate JSON keys, and every value above a recorded
//! bound before typed decoding.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The exact contract version of every issue #113 family artifact.
pub const VERSION: &str = "0.8.0";
/// The model contract version the family projects from.
pub const MODEL_VERSION: &str = "0.2.16";
/// The diagnostic registry release the boundary families pin.
pub const REGISTRY_VERSION: &str = "0.8.0";

/// The attachment family identities.
pub const API_IDENTITY: &str = "dev.lekalo.module-api";
pub const API_SCHEMA: &str = "lekalo/module-api";
/// The public bundle family identities.
pub const PUBLIC_IDENTITY: &str = "dev.lekalo.module-public-contract";
pub const PUBLIC_SCHEMA: &str = "lekalo/module-public-contract";
/// The boundary report family identities.
pub const REPORT_IDENTITY: &str = "dev.lekalo.module-api-report";
pub const REPORT_SCHEMA: &str = "lekalo/module-api-report";
/// The consumer snapshot family identities.
pub const CONSUMERS_IDENTITY: &str = "dev.lekalo.module-api-consumers";
pub const CONSUMERS_SCHEMA: &str = "lekalo/module-api-consumers";
/// The diff family identities.
pub const DIFF_IDENTITY: &str = "dev.lekalo.module-api-diff";
pub const DIFF_SCHEMA: &str = "lekalo/module-api-diff";
/// The consumer lock family identities.
pub const LOCK_IDENTITY: &str = "dev.lekalo.module-api-lock";
pub const LOCK_SCHEMA: &str = "lekalo/module-api-lock";
/// The consumer context family identities.
pub const CONTEXT_IDENTITY: &str = "dev.lekalo.module-consumer-context";
pub const CONTEXT_SCHEMA: &str = "lekalo/module-consumer-context";

/// The distinct digest domains of the family.
pub const DIGEST_DOMAIN_API: &str = "lekalo/module-api/1";
pub const DIGEST_DOMAIN_PUBLIC: &str = "lekalo/module-public-contract/1";
pub const DIGEST_DOMAIN_CONSUMERS: &str = "lekalo/module-api-consumers/1";
pub const DIGEST_DOMAIN_CONTEXT: &str = "lekalo/module-consumer-context/1";

/// Recorded hard bounds of the family (issue #113 proposal).
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_MODULES: usize = 2_000;
pub const MAX_EXPORTS_PER_MODULE: usize = 4_096;
pub const MAX_USES_PER_CONSUMER: usize = 4_096;
pub const MAX_DEPENDENCY_NESTING: usize = 256;
pub const MAX_CONTEXT_ROOTS: usize = 128;
pub const MAX_CONTEXT_TOKENS: u64 = 1_000_000;
pub const MAX_REPORT_BYTES: usize = 32 * 1024 * 1024;
/// The documented offline estimator of the consumer context family.
pub const ESTIMATOR_ID: &str = "module-context-estimator";
pub const ESTIMATOR_RECIPE: &str = "public-facts-chars4/1";
pub const ESTIMATOR_VERSION: &str = "1";

/// Canonical bytes: compact UTF-8 JSON with byte-sorted object keys.
pub fn canonical(value: &impl Serialize) -> String {
    let value = serde_json::to_value(value).expect("typed value serializes");
    write_canonical(&value)
}

/// Recursive canonical writer over a decoded JSON value.
pub fn write_canonical(value: &serde_json::Value) -> String {
    let mut out = String::new();
    match value {
        serde_json::Value::Null => out.push_str("null"),
        serde_json::Value::Bool(true) => out.push_str("true"),
        serde_json::Value::Bool(false) => out.push_str("false"),
        serde_json::Value::Number(number) => {
            out.push_str(&number.to_string());
        }
        serde_json::Value::String(text) => {
            write_json_string(text, &mut out);
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (position, item) in items.iter().enumerate() {
                if position > 0 {
                    out.push(',');
                }
                out.push_str(&write_canonical(item));
            }
            out.push(']');
        }
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
            out.push('{');
            for (position, key) in keys.iter().enumerate() {
                if position > 0 {
                    out.push(',');
                }
                write_json_string(key, &mut out);
                out.push(':');
                out.push_str(&write_canonical(&map[*key]));
            }
            out.push('}');
        }
    }
    out
}

/// RFC 8259 mandatory-escape string writer (the loader's exact spelling).
pub(crate) fn write_json_string(text: &str, out: &mut String) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// SHA-256 over a digest domain prefix plus canonical bytes.
pub fn digest(domain: &str, canonical_bytes: &str) -> String {
    format!(
        "sha256:{}",
        crate::digest::sha256_hex(format!("{domain}\n{canonical_bytes}").as_bytes())
    )
}

/// Strict decode: size bound, duplicate-key rejection, closed shape, and
/// no trailing bytes — the same fail-closed chain every family uses.
pub fn decode<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    detail: fn(&str) -> crate::diagnostics::DiagnosticSet,
) -> Result<T, crate::diagnostics::DiagnosticSet> {
    if bytes.len() > MAX_BYTES {
        return Err(detail("input-size"));
    }
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let value = Unique::deserialize(&mut de)
        .map_err(|_| detail("json-shape"))?
        .0;
    de.end().map_err(|_| detail("json-trailing"))?;
    serde_json::from_value(value).map_err(|_| detail("closed-shape"))
}

struct Unique(serde_json::Value);

impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("unique-key JSON")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(serde_json::Value::Null))
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|v| Unique(v.into()))
                    .ok_or_else(|| E::custom("number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(v) = a.next_element::<Unique>()? {
                    values.push(v.0);
                }
                Ok(Unique(values.into()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if values.contains_key(&k) {
                        return Err(serde::de::Error::custom("duplicate-key"));
                    }
                    values.insert(k, a.next_value::<Unique>()?.0);
                }
                Ok(Unique(values.into()))
            }
        }
        d.deserialize_any(V)
    }
}

/// The `module-api` attachment document (source seam).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Attachment {
    pub schema_version: String,
    pub identity: String,
    pub project: String,
    pub modules: Vec<ModulePolicy>,
}

/// The policy of one exact semantic module.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModulePolicy {
    pub module_id: String,
    pub api_version: Option<String>,
    #[serde(default)]
    pub exports: Vec<ExportEntry>,
    #[serde(default)]
    pub imports: Vec<ImportPolicy>,
    #[serde(default)]
    pub consumers: Vec<ConsumerDeclaration>,
    #[serde(default)]
    pub ports: Vec<PortPolicy>,
    #[serde(default)]
    pub capabilities: Vec<CapabilityPolicy>,
    #[serde(default)]
    pub grants: Vec<GrantPolicy>,
    #[serde(default)]
    pub deprecations: Vec<DeprecationPolicy>,
}

/// One explicit export allowlist row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportEntry {
    pub export: String,
    pub kind: String,
    pub since: Option<String>,
    pub alias_of: Option<String>,
}

/// One declared import with its selected public uses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportPolicy {
    pub module_id: String,
    #[serde(default)]
    pub uses: Vec<String>,
}

/// One declared consumer of this producer module.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsumerDeclaration {
    pub consumer: String,
    pub uses: Vec<String>,
    pub mode: String,
    pub policy: Option<VersionPolicyWire>,
}

/// The bounded independent version policy vocabulary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum VersionPolicyWire {
    ExactDigest { digest: String },
    ExactVersion { version: String },
    VersionRange {
        lower: String,
        lower_inclusive: bool,
        upper: Option<String>,
        upper_inclusive: bool,
    },
}

/// One public service-port descriptor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortPolicy {
    pub port: String,
    pub operations: Vec<String>,
}

/// One capability requirement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityPolicy {
    pub capability: String,
    pub obligation: String,
    pub owner: Option<String>,
    pub version: Option<String>,
}

/// One explicit shared-aggregate write grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantPolicy {
    pub grant: String,
    pub consumer: String,
    pub aggregate: String,
    pub operations: Vec<String>,
    pub fields: Vec<String>,
    #[serde(default)]
    pub invariant: String,
    pub rationale: String,
}

/// One declared deprecation window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeprecationPolicy {
    pub export: String,
    pub introduced_in: String,
    pub removed_in: Option<String>,
    pub replacement: Option<String>,
    #[serde(default)]
    pub hint: String,
}

/// The standalone public bundle of one producer module.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicBundle {
    pub schema_version: String,
    pub identity: String,
    pub producer: ProducerPin,
    pub api_version: Option<String>,
    pub digest: String,
    pub pins: PublicPins,
    pub exports: Vec<PublicExport>,
    #[serde(default)]
    pub public_dependencies: Vec<PublicDependency>,
    #[serde(default)]
    pub ports: Vec<PortPolicy>,
    #[serde(default)]
    pub capabilities: Vec<CapabilityPolicy>,
    #[serde(default)]
    pub deprecations: Vec<DeprecationPolicy>,
}

/// The producer identity of a standalone artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProducerPin {
    pub project: String,
    pub module: String,
}

/// Freshness pins of a public bundle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicPins {
    pub model_version: String,
    pub ir_digest: String,
    pub source_revision: State<String>,
}

/// A `known`/`unknown` state that never carries an unavailable scalar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
pub enum State<T> {
    Known { value: T },
    Unknown,
}

impl<T> State<T> {
    pub fn known(value: T) -> Self {
        Self::Known { value }
    }
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Known { value } => Some(value),
            Self::Unknown => None,
        }
    }
}

/// One exported member of a public bundle: the closed export surface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicExport {
    pub export: String,
    pub kind: String,
    pub version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<PublicField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identity: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<TypeExpr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reads: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input: Vec<PublicField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payload: Vec<PublicField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectObligation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invokes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
}

impl PublicExport {
    /// One typed, otherwise-empty export row with its identity and kind.
    pub fn empty(export: &str, kind: &str) -> Self {
        Self {
            export: export.to_owned(),
            kind: kind.to_owned(),
            version: 0,
            base: None,
            values: Vec::new(),
            fields: Vec::new(),
            identity: Vec::new(),
            returns: None,
            reads: Vec::new(),
            input: Vec::new(),
            payload: Vec::new(),
            effects: Vec::new(),
            code: None,
            retryable: None,
            operations: Vec::new(),
            invokes: None,
            method: None,
            path: None,
            decision: None,
            operation: None,
            entity: None,
            emits: Vec::new(),
        }
    }
}

/// One public field with its exact type expression.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicField {
    pub name: String,
    pub r#type: TypeExpr,
    pub required: bool,
}

/// The closed public type expression: `ref`, `list`, or `optional`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, rename_all = "camelCase", deny_unknown_fields)]
pub enum TypeExpr {
    Ref { r#ref: String },
    List { list: Box<TypeExpr> },
    Optional { optional: Box<TypeExpr> },
}

impl TypeExpr {
    /// The leaf symbol id of the expression.
    pub fn leaf(&self) -> &str {
        match self {
            Self::Ref { r#ref } => r#ref,
            Self::List { list } => list.leaf(),
            Self::Optional { optional } => optional.leaf(),
        }
    }
    /// The nullable wrapper is transparent for requiredness semantics.
    pub fn is_optional(&self) -> bool {
        matches!(self, Self::Optional { .. })
    }
}

/// One observable effect obligation of an exported command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectObligation {
    pub entity: String,
    pub operation: String,
    pub emits: Vec<String>,
}

/// One exact public dependency pin (foreign exported symbols used).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicDependency {
    pub module_id: String,
    pub digest: String,
    pub api_version: Option<String>,
    pub exports: Vec<String>,
}

/// The boundary assessment report of `module api check`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Report {
    pub schema_version: String,
    pub identity: String,
    pub assessment: String,
    pub command: String,
    pub strict: bool,
    pub module_filter: Option<String>,
    pub pins: ReportPins,
    pub modules: Vec<ModuleSummary>,
    pub findings: Vec<Finding>,
    pub cycles: Vec<CycleWitness>,
    pub gaps: Vec<String>,
    pub complete: bool,
}

/// The pins of a boundary report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportPins {
    pub project: String,
    pub model_version: String,
    pub ir_digest: String,
    pub attachment_digest: String,
    pub registry_version: String,
}

/// One module row of the boundary report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleSummary {
    pub module_id: String,
    pub api_version: Option<String>,
    pub attachment: bool,
    pub exports: u64,
    pub imports: Vec<String>,
    pub consumers: u64,
    pub public_digest: Option<String>,
}

/// One typed boundary finding (the report projection of a diagnostic).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Finding {
    pub rule: String,
    pub code: String,
    pub severity: String,
    pub category: String,
    pub subject: String,
    pub detail: String,
    pub consumer: Option<String>,
    pub producer: Option<String>,
    pub export: Option<String>,
    pub member: Option<String>,
    pub path: Vec<String>,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

/// One cycle witness with its canonical path and original edge keys.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CycleWitness {
    pub projection: String,
    pub members: Vec<String>,
    pub path: Vec<String>,
    pub edges: Vec<String>,
}

/// The consumer snapshot of one producer revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsumerSnapshot {
    pub schema_version: String,
    pub identity: String,
    pub producer: ProducerPin,
    pub pins: SnapshotPins,
    pub consumers: Vec<ConsumerRow>,
    pub complete: bool,
    pub digest: String,
}

/// The pins of a consumer snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotPins {
    pub project: String,
    pub model_version: String,
    pub ir_digest: String,
    pub api_digest: String,
}

/// One consumer row: declared and resolved facts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsumerRow {
    pub consumer: String,
    pub declared: bool,
    pub mode: Option<String>,
    pub policy: Option<VersionPolicyWire>,
    pub uses: Vec<ConsumerUse>,
}

/// One exact use of an exported symbol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsumerUse {
    pub export: String,
    pub member: Option<String>,
    pub origin: String,
    pub sensitivity: String,
    pub transitive: bool,
    pub edges: Vec<String>,
}

/// The public diff of two bundles of the same module.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiffReport {
    pub schema_version: String,
    pub identity: String,
    pub before: DiffSide,
    pub after: DiffSide,
    pub changes: Vec<DiffChange>,
    pub consumers: Vec<ConsumerImpact>,
    pub coverage: DiffCoverage,
    pub version_policy: VersionDecision,
    pub verdict: String,
}

/// One side of a public diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiffSide {
    pub module: String,
    pub project: String,
    pub digest: String,
    pub api_version: Option<String>,
}

/// One classified public change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiffChange {
    pub export: String,
    pub member: Option<String>,
    pub class: String,
    pub reason: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// The affected/compatible/unaffected/unknown partition of one consumer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsumerImpact {
    pub consumer: String,
    pub status: String,
    pub present_in: String,
    pub policy: Option<String>,
    pub changes: Vec<String>,
    pub witness: Vec<String>,
    pub reason: Option<String>,
}

/// The explicit coverage domain of a diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiffCoverage {
    pub known: u64,
    pub possible: Vec<String>,
    pub unknown_reasons: Vec<String>,
}

/// The version-policy decision of a diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionDecision {
    pub decision: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub detail: String,
}

/// The consumer lock of one public resolution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsumerLock {
    pub schema_version: String,
    pub identity: String,
    pub consumer: String,
    pub producer: ProducerPin,
    pub api_version: Option<String>,
    pub digest: String,
    pub policy: Option<VersionPolicyWire>,
    pub public_dependencies: Vec<PublicDependency>,
    pub selected: Vec<String>,
}

/// The consumer context capsule built from a public bundle only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextCapsule {
    pub schema_version: String,
    pub identity: String,
    pub consumer: String,
    pub producer: ProducerPin,
    pub producer_digest: String,
    pub api_version: Option<String>,
    pub budget: u64,
    pub estimator: EstimatorPin,
    pub fits: bool,
    pub required_cost: u64,
    pub selected_cost: u64,
    pub exports: Vec<ExportFacts>,
    pub dependencies: Vec<PublicDependency>,
    pub omissions: Vec<Omission>,
    pub gaps: Vec<String>,
    pub digest: String,
}

/// The pinned offline estimator identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EstimatorPin {
    pub id: String,
    pub recipe: String,
    pub version: String,
}

/// The public facts of one selected export.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportFacts {
    pub export: String,
    pub kind: String,
    pub signature: String,
    pub errors: Vec<String>,
    pub effects: Vec<String>,
    pub capabilities: Vec<String>,
    pub deprecation: Option<DeprecationPolicy>,
    pub api_version: Option<String>,
}

/// One omitted optional fact with its reason.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Omission {
    pub subject: String,
    pub reason: String,
}

/// BTree convenience alias for the resolved attachment map.
pub type ModuleMap = BTreeMap<String, ModulePolicy>;

#[cfg(test)]
mod probe_tests {
    use super::*;
    #[test]
    fn probe_attachment_decode() {
        let bytes =
            std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/module-api/reference/tasks/module-api.json")).unwrap();
        let value: Result<Attachment, _> = serde_json::from_slice(&bytes);
        match value {
            Ok(a) => println!("OK modules={}", a.modules.len()),
            Err(e) => println!("ERR {e}"),
        }
    }
}
