//! Closed coupling contracts. Unavailable values cannot carry a scalar.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const VERSION: &str = "0.6.4";
pub const METRIC_VERSION: &str = "coupling-metrics/1";
pub const RECIPE: &str = "declared-architecture/1";
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_SUBJECTS: usize = 2_000;
pub const METRICS: &[&str] = &[
    "fanInSymbols",
    "fanOutSymbols",
    "fanInModules",
    "fanOutModules",
    "publicContractsAffected",
    "internalSymbolsAffected",
    "semanticSymbolsAffected",
    "modulesAffected",
    "affectedArtifacts",
    "affectedTests",
    "affectedTargets",
    "requiredChecks",
    "crossModuleCycles",
    "sharedMutableResources",
    "transactionSpread",
    "sharedAbstractionRadius",
    "publicTargetExposure",
    "duplicationDivergence",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
pub enum State<T> {
    Known { value: T },
    Unknown,
    Withheld,
    Unsupported,
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for State<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Known<T> {
            state: KnownTag,
            value: T,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        enum KnownTag {
            Known,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Unavailable {
            state: UnavailableTag,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        enum UnavailableTag {
            Unknown,
            Withheld,
            Unsupported,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw<T> {
            Known(Known<T>),
            Unavailable(Unavailable),
        }
        Ok(match Raw::deserialize(d)? {
            Raw::Known(Known {
                state: KnownTag::Known,
                value,
            }) => State::Known { value },
            Raw::Unavailable(Unavailable {
                state: UnavailableTag::Unknown,
            }) => State::Unknown,
            Raw::Unavailable(Unavailable {
                state: UnavailableTag::Withheld,
            }) => State::Withheld,
            Raw::Unavailable(Unavailable {
                state: UnavailableTag::Unsupported,
            }) => State::Unsupported,
        })
    }
}
impl<T> State<T> {
    pub fn known(value: T) -> Self {
        Self::Known { value }
    }
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Known { value } => Some(value),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scope {
    pub kind: String,
    pub selector: String,
    pub roots: Vec<String>,
    pub changed_input: State<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pins {
    pub project: String,
    pub model_version: String,
    pub source_revision: State<String>,
    pub model_digest: State<String>,
    pub ir_digest: String,
    pub semantic_digest: String,
    pub graph_digest: String,
    pub effect_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Provenance {
    pub inputs: Pins,
    pub evidence: State<String>,
    pub profile_digest: String,
    pub measurement_digest: String,
    pub projection_digest: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SymbolFact {
    pub module: String,
    pub class: String,
    pub target_specific: bool,
    pub effect_operation: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Snapshot {
    pub symbols: BTreeMap<String, SymbolFact>,
    pub edges: Vec<Edge>,
    pub fields: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Partition {
    pub public_contracts: Vec<String>,
    pub internal_symbols: Vec<String>,
    pub supporting_symbols: Vec<String>,
    pub unclassified_symbols: Vec<String>,
}
impl Partition {
    pub fn ids(&self) -> impl Iterator<Item = &String> {
        self.public_contracts
            .iter()
            .chain(&self.internal_symbols)
            .chain(&self.supporting_symbols)
            .chain(&self.unclassified_symbols)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Subject {
    pub subject: String,
    pub module: String,
    pub measurement_basis: String,
    pub metrics: BTreeMap<String, State<u64>>,
    pub impact: Partition,
    pub possible: Vec<String>,
    pub lower_bounds: BTreeMap<String, u64>,
    pub fan_in: Vec<String>,
    pub fan_out: Vec<String>,
    pub affected_modules: Vec<String>,
    pub artifacts: Vec<String>,
    pub tests: Vec<String>,
    pub targets: Vec<String>,
    pub checks: Vec<String>,
    pub resources: Vec<String>,
    pub shared_resources: Vec<String>,
    pub target_exposure: Vec<String>,
    pub replica_obligations: Vec<String>,
    pub transaction_groups: Vec<String>,
    pub transaction_scopes: Vec<TransactionScope>,
    pub witness_refs: Vec<String>,
    pub gaps: Vec<String>,
    pub import_edge_keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edge {
    pub key: String,
    pub from: String,
    pub to: String,
    pub relation: String,
    pub occurrence: u32,
    pub role: String,
    pub confidence: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransactionScope {
    pub group: String,
    pub modules: Vec<String>,
    pub domain_groups: Vec<String>,
    pub classified: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Witness {
    pub id: String,
    pub root: String,
    pub subject: String,
    pub direction: String,
    pub edges: Vec<Edge>,
    pub evidence_refs: Vec<String>,
    pub confidence: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cycle {
    pub series: String,
    pub members: Vec<String>,
    pub modules: Vec<String>,
    pub edge_keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Finding {
    pub rule: String,
    pub subject: String,
    pub metric: String,
    pub detail: String,
    pub witness_refs: Vec<String>,
    pub centrality: State<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Suggestion {
    pub subject: String,
    pub action: String,
    pub witness_refs: Vec<String>,
    pub preserve: Vec<String>,
    pub applied: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Planning {
    pub public_contracts: Vec<String>,
    pub internal_symbols: Vec<String>,
    pub resources: Vec<String>,
    pub transaction_groups: Vec<String>,
    pub required_checks: Vec<String>,
    pub evidence_gaps: Vec<String>,
    pub context_budget: State<String>,
    pub runtime_conflicts: State<String>,
    pub review_overlap: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Report {
    pub schema_version: String,
    pub identity: String,
    pub metric_version: String,
    pub scope: Scope,
    pub recipe: String,
    pub profile: super::profile::Profile,
    pub provenance: Provenance,
    pub projection: Snapshot,
    pub coverage: BTreeMap<String, State<String>>,
    pub subjects: Vec<Subject>,
    pub summary: Partition,
    pub public_contract_counts: BTreeMap<String, u64>,
    pub cycles: Vec<Cycle>,
    pub witnesses: Vec<Witness>,
    pub findings: Vec<Finding>,
    pub suggestions: Vec<Suggestion>,
    pub planning: Planning,
    pub complete: bool,
}

pub fn digest<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(&serde_json::to_value(value).expect("typed coupling value"))
        .expect("typed coupling JSON");
    format!("sha256:{}", crate::digest::sha256_hex(&bytes))
}
pub fn is_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Recursive duplicate-key refusal before typed decoding; serde_json::Value alone
/// would discard this evidence. Null is outside all coupling optional members.
pub fn decode<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
) -> Result<T, crate::diagnostics::DiagnosticSet> {
    if bytes.len() > MAX_BYTES {
        return Err(super::diagnostic::invalid("input-size"));
    }
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let value = Unique::deserialize(&mut de)
        .map_err(|_| super::diagnostic::invalid("json-shape"))?
        .0;
    de.end()
        .map_err(|_| super::diagnostic::invalid("json-trailing"))?;
    serde_json::from_value(value).map_err(|_| super::diagnostic::invalid("closed-shape"))
}
struct Unique(serde_json::Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("non-null unique-key JSON")
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
