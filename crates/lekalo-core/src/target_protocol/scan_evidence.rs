//! Wire data shared by all generic scan evidence families.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const MAX_GROUPS: usize = 16;
pub(crate) const MAX_FACTS: usize = 2048;
const MAX_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Coverage {
    Complete,
    Partial,
    Unassessed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub family: String,
    pub package_digest: String,
    pub schema_digest: String,
    pub coverage: Coverage,
    pub facts: Vec<Fact>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    pub subject: String,
    pub key: String,
    pub references: BTreeMap<String, String>,
    pub sources: Vec<Source>,
    pub payload: Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub path: String,
    pub source_digest: String,
}

pub(crate) fn valid_groups(groups: &[Group]) -> bool {
    let mut families = BTreeSet::new();
    groups.len() <= MAX_GROUPS
        && groups.iter().map(|g| g.facts.len()).sum::<usize>() <= MAX_FACTS
        && groups.iter().all(|g| {
            super::scopes::is_token(&g.family)
                && families.insert(&g.family)
                && super::wire::is_sha256_digest(&g.package_digest)
                && super::wire::is_sha256_digest(&g.schema_digest)
                && g.coverage != Coverage::Unassessed
                && g.facts.iter().all(Fact::valid)
        })
}

impl Fact {
    pub(crate) fn valid(&self) -> bool {
        fn key(s: &str) -> bool {
            !s.is_empty() && s.len() <= 256 && s.trim() == s && !s.chars().any(char::is_control)
        }

        key(&self.subject)
            && key(&self.key)
            && self.references.len() <= 16
            && self
                .references
                .iter()
                .all(|(role, native)| super::scopes::is_token(role) && key(native))
            && (1..=64).contains(&self.sources.len())
            && self.sources.iter().all(|s| {
                super::scopes::is_logical_path(&s.path)
                    && super::wire::is_sha256_digest(&s.source_digest)
            })
            && self
                .sources
                .iter()
                .map(|s| &s.path)
                .collect::<BTreeSet<_>>()
                .len()
                == self.sources.len()
            && serde_json::to_vec(&self.payload).is_ok_and(|bytes| bytes.len() <= MAX_PAYLOAD_BYTES)
    }

    pub(crate) fn anchors_match(&self, paths: &BTreeMap<&str, &str>) -> bool {
        std::iter::once(&self.subject)
            .chain(self.references.values())
            .all(|native| {
                paths
                    .get(native.as_str())
                    .is_some_and(|path| self.sources.iter().any(|source| source.path == *path))
            })
    }
}
