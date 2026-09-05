//! The closed record envelope and its validation (issue #20).
//!
//! Every persisted record is self-describing and rejects unknown fields,
//! unsupported versions, malformed digests, mixed bindings, and unbounded
//! arrays before any typed allocation: a record that fails validation is a
//! miss, never a trusted hit.

use serde::{Deserialize, Serialize};

use super::canonical::{canonical_bytes, is_digest, sha256_digest};
use super::key::{is_sorted_unique, Key, RecordKind};
use super::version;

/// The closed dependency-edge role vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DependencyRole {
    Input,
    Upstream,
    Producer,
    Profile,
    Lock,
    Adapter,
}

/// One typed dependency edge: the downstream entry depends on the entry
/// whose key digest is `key_digest`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DependencyEdge {
    pub(crate) key_digest: String,
    pub(crate) role: DependencyRole,
    pub(crate) ordinal: u64,
}

impl DependencyRole {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Upstream => "upstream",
            Self::Producer => "producer",
            Self::Profile => "profile",
            Self::Lock => "lock",
            Self::Adapter => "adapter",
        }
    }
}

/// One consumed producer-contract reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContractRef {
    pub(crate) family: String,
    pub(crate) identity: String,
}

/// The exact binding vector every record carries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Binding {
    pub(crate) cache_identity: String,
    pub(crate) contracts: Vec<ContractRef>,
    pub(crate) pipeline_revision: u64,
}

impl Binding {
    /// The binding of the mirrored decode pipeline (parsed fragments).
    pub(crate) fn pipeline() -> Self {
        Self {
            cache_identity: version::IDENTITY.to_owned(),
            contracts: Vec::new(),
            pipeline_revision: version::PIPELINE_REVISION,
        }
    }

    /// The binding of one producer fragment over accepted contracts.
    pub(crate) fn producer<'a>(contracts: &[(&'a str, &'a str)]) -> Self {
        Self {
            cache_identity: version::IDENTITY.to_owned(),
            contracts: contracts
                .iter()
                .map(|(family, identity)| ContractRef {
                    family: (*family).to_owned(),
                    identity: (*identity).to_owned(),
                })
                .collect(),
            pipeline_revision: version::PIPELINE_REVISION,
        }
    }

    /// Whether this binding exactly matches the running implementation.
    pub(crate) fn is_current(&self) -> bool {
        if self.cache_identity != version::IDENTITY
            || self.pipeline_revision != version::PIPELINE_REVISION
        {
            return false;
        }
        self.contracts.iter().all(|contract| {
            contract.family.starts_with("dev.lekalo.")
                && contract.identity.contains('@')
                && contract.identity.len() <= 160
        })
    }
}

/// One bounded integer fact of the producer snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PayloadField {
    pub(crate) name: String,
    pub(crate) value: u64,
}

/// The bounded typed payload value: a digest plus bounded counts only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PayloadValue {
    pub(crate) digest: String,
    pub(crate) fields: Vec<PayloadField>,
}

impl PayloadValue {
    /// Build a payload whose fields are normalized to sorted, unique names.
    pub(crate) fn new(digest: String, mut fields: Vec<PayloadField>) -> Self {
        fields.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
        fields.dedup_by(|left, right| left.name == right.name);
        Self { digest, fields }
    }

    /// The digest over the canonical payload-value bytes.
    pub(crate) fn digest(&self) -> String {
        sha256_digest(&canonical_bytes(self))
    }

    pub(crate) fn is_well_formed(&self) -> bool {
        is_digest(&self.digest)
            && self.fields.len() <= 64
            && self.fields.iter().all(|field| {
                !field.name.is_empty()
                    && field.name.len() <= 64
                    && field.name.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte == b'-' || byte.is_ascii_digit()
                    })
            })
    }
}

/// The bounded payload wrapper.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Payload {
    pub(crate) encoding: String,
    pub(crate) value: PayloadValue,
}

/// The closed record envelope: the only shape the store persists.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Record {
    pub(crate) schema_version: String,
    pub(crate) identity: String,
    pub(crate) record_kind: RecordKind,
    pub(crate) key: Key,
    pub(crate) key_digest: String,
    pub(crate) binding: Binding,
    pub(crate) dependencies: Vec<DependencyEdge>,
    pub(crate) payload: Payload,
    pub(crate) payload_digest: String,
}

/// Maximum dependency edges accepted per record.
pub(crate) const MAX_DEPENDENCIES: usize = 1024;

impl Record {
    /// Assemble one record from typed parts; dependencies are normalized
    /// to sorted, unique `(key_digest, role, ordinal)` order.
    pub(crate) fn new(
        kind: RecordKind,
        key: Key,
        binding: Binding,
        payload_value: PayloadValue,
    ) -> Self {
        Self {
            schema_version: version::SCHEMA_VERSION.to_owned(),
            identity: version::IDENTITY.to_owned(),
            key_digest: String::new(),
            record_kind: kind,
            key,
            binding,
            dependencies: Vec::new(),
            payload: Payload {
                encoding: "lekalo/canonical-json".to_owned(),
                value: payload_value,
            },
            payload_digest: String::new(),
        }
    }

    /// Attach one `input` dependency edge with the next ordinal.
    pub(crate) fn with_input(mut self, key_digest: String) -> Self {
        let ordinal = self
            .dependencies
            .iter()
            .filter(|edge| edge.role == DependencyRole::Input)
            .count() as u64;
        self.dependencies.push(DependencyEdge {
            key_digest,
            role: DependencyRole::Input,
            ordinal,
        });
        self
    }

    /// Seal the record: normalize the dependency order and compute the
    /// payload digest over the canonical payload bytes.
    pub(crate) fn sealed(mut self) -> Self {
        self.dependencies.sort_by(|left, right| {
            left.key_digest
                .as_bytes()
                .cmp(right.key_digest.as_bytes())
                .then(left.role.as_str().cmp(right.role.as_str()))
                .then(left.ordinal.cmp(&right.ordinal))
        });
        self.dependencies.dedup();
        self.dependencies.dedup();
        self.key_digest = self.key.digest();
        self.payload_digest = self.payload.value.digest();
        self
    }

    /// Whether the pinned key digest matches the key bytes.
    pub(crate) fn key_digest_is_consistent(&self) -> bool {
        self.key_digest == self.key.digest()
    }

    /// Full validation before typed use: envelope identity, key and
    /// payload well-formedness, binding currency, dependency bounds and
    /// order, and the payload digest itself.
    pub(crate) fn validate(&self) -> bool {
        if self.schema_version != version::SCHEMA_VERSION || self.identity != version::IDENTITY {
            return false;
        }
        if self.record_kind != self.key.kind() {
            return false;
        }
        if !self.key.is_well_formed() || !self.binding.is_current() {
            return false;
        }
        if !self.key_digest_is_consistent() {
            return false;
        }
        if !self.payload.value.is_well_formed() {
            return false;
        }
        if self.dependencies.len() > MAX_DEPENDENCIES {
            return false;
        }
        let digests: Vec<String> = self
            .dependencies
            .iter()
            .map(|edge| edge.key_digest.clone())
            .collect();
        if !is_sorted_unique(&digests) {
            return false;
        }
        if !self
            .dependencies
            .iter()
            .all(|edge| is_digest(&edge.key_digest))
        {
            return false;
        }
        if self.payload_digest != self.payload.value.digest() {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEX1: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

    fn sample_record() -> Record {
        let key = Key::Source {
            path: "lekalo/project.yaml".to_owned(),
            raw_digest: HEX1.to_owned(),
            normalized_digest: crate::cache::canonical::sha256_digest(b"normalized"),
        };
        Record::new(
            RecordKind::Source,
            key,
            Binding::pipeline(),
            PayloadValue::new(HEX1.to_owned(), Vec::new()),
        )
        .sealed()
    }

    #[test]
    fn sealed_records_validate_and_carry_exact_digests() {
        let record = sample_record();
        assert!(record.validate());
        assert_eq!(record.payload_digest, record.payload.value.digest());
    }

    #[test]
    fn corrupted_digests_are_rejected_before_typed_use() {
        let mut record = sample_record();
        record.payload_digest = HEX1.replace('1', "2");
        assert!(!record.validate());
        let mut record = sample_record();
        record.schema_version = "lekalo/cache/v9.9.9".to_owned();
        assert!(!record.validate());
        let mut record = sample_record();
        record.record_kind = RecordKind::ParsedFragment;
        assert!(!record.validate());
    }

    #[test]
    fn dependencies_are_normalized_sorted_and_deduplicated() {
        let record = sample_record()
            .with_input(
                "sha256:2222222222222222222222222222222222222222222222222222222222222222"
                    .to_owned(),
            )
            .with_input(
                "sha256:aaaa222222222222222222222222222222222222222222222222222222222222"
                    .to_owned(),
            )
            .with_input(HEX1.to_owned())
            .sealed();
        assert!(record.validate());
        let digests: Vec<&str> = record
            .dependencies
            .iter()
            .map(|edge| edge.key_digest.as_str())
            .collect();
        let mut sorted = digests.clone();
        sorted.sort_unstable();
        assert_eq!(digests, sorted);
    }

    #[test]
    fn stale_pipeline_bindings_never_validate() {
        let mut record = sample_record();
        record.binding.pipeline_revision = version::PIPELINE_REVISION + 1;
        assert!(!record.validate());
        let mut record = sample_record();
        record.binding.cache_identity = "dev.lekalo.cache@0.0.1".to_owned();
        assert!(!record.validate());
    }
}
