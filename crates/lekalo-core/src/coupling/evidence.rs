//! Existing evidence owners validate their own bytes before coupling joins them.
use super::{
    diagnostic,
    wire::{self, Pins, State},
};
use crate::diagnostics::DiagnosticSet;
use crate::effects::EffectGraph;
use crate::ir::{CompiledProject, Definition};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Attachment {
    pub digest: String,
    pub bytes: String,
}
impl Attachment {
    fn verify(&self) -> Result<(), DiagnosticSet> {
        if self.bytes.len() > 16 * 1024 * 1024
            || self.digest
                != format!(
                    "sha256:{}",
                    crate::digest::sha256_hex(self.bytes.as_bytes())
                )
        {
            return Err(diagnostic::invalid("attachment-digest"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Replica {
    pub obligation: String,
    pub subjects: Vec<String>,
    pub review_ref: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Evidence {
    pub schema_version: String,
    pub identity: String,
    pub inputs: Pins,
    pub artifacts: State<Attachment>,
    pub trace: State<Attachment>,
    pub queries: State<Attachment>,
    pub transactions: State<Attachment>,
    pub replicas: Vec<Replica>,
}
pub struct Joined {
    pub digest: State<String>,
    pub artifacts: Option<crate::artifacts::types::ArtifactManifest>,
    pub trace: Option<crate::trace::TraceManifest>,
    pub queries: Option<crate::query_model::QueryModelAttachment>,
    pub transactions: Option<crate::transaction_concurrency::TransactionConcurrencyAttachment>,
    pub replica_divergences: BTreeMap<String, Vec<String>>,
    pub replica_subjects: BTreeSet<String>,
}
impl Default for Joined {
    fn default() -> Self {
        Self {
            digest: State::Unknown,
            artifacts: None,
            trace: None,
            queries: None,
            transactions: None,
            replica_divergences: BTreeMap::new(),
            replica_subjects: BTreeSet::new(),
        }
    }
}
impl Evidence {
    pub fn parse(bytes: &[u8]) -> Result<Self, DiagnosticSet> {
        let v: Self = wire::decode(bytes)?;
        if v.schema_version != format!("lekalo/coupling-evidence/v{}", wire::VERSION)
            || v.identity != format!("dev.lekalo.coupling-evidence@{}", wire::VERSION)
        {
            return Err(diagnostic::unsupported("evidence-version"));
        }
        if v.replicas.len() > 256 {
            return Err(diagnostic::invalid("replica-limit"));
        }
        Ok(v)
    }
    pub fn join(
        &self,
        pins: &Pins,
        project: &CompiledProject,
        effects: &EffectGraph,
    ) -> Result<Joined, DiagnosticSet> {
        if &self.inputs != pins {
            return Err(diagnostic::invalid("evidence-input-pins"));
        }
        let mut joined = Joined {
            digest: State::known(wire::digest(self)),
            ..Joined::default()
        };
        if let Some(a) = self.artifacts.value() {
            a.verify()?;
            let m = crate::artifacts::types::ArtifactManifest::parse_canonical(a.bytes.as_bytes())
                .map_err(|_| diagnostic::invalid("artifact-owner"))?;
            if m.project().as_str() != pins.project
                || m.ir().digest().as_str() != pins.ir_digest
                || pins.model_digest.value().map(String::as_str)
                    != Some(m.model().digest().as_str())
                || m.model().version().as_str() != pins.model_version
            {
                return Err(diagnostic::invalid("artifact-pins"));
            }
            let ids: BTreeSet<_> = project
                .definitions
                .iter()
                .map(|d| d.id().as_str())
                .chain(project.modules.iter().map(|m| m.id.as_str()))
                .collect();
            if m.artifacts().iter().any(|a| {
                !ids.contains(a.key().semantic_owner().as_str())
                    || a.input_refs().iter().any(|i| !ids.contains(i.as_str()))
            }) {
                return Err(diagnostic::invalid("artifact-semantic-id"));
            }
            joined.artifacts = Some(m);
        }
        if let Some(a) = self.trace.value() {
            a.verify()?;
            let trace = crate::trace::TraceManifest::parse(a.bytes.as_bytes())?;
            let m = trace.manifest();
            if m.project_ref != pins.project
                || pins.model_digest.value() != Some(&m.model_ref.digest)
                || m.ir_ref.as_ref().map(|r| r.digest.as_str()) != Some(pins.ir_digest.as_str())
                || m.graph_ref.as_ref().map(|r| r.digest.as_str())
                    != Some(pins.graph_digest.as_str())
                || m.artifact_manifest_ref.as_ref().is_some_and(|r| {
                    joined
                        .artifacts
                        .as_ref()
                        .map(|m| m.manifest_digest().as_str())
                        != Some(r.digest.as_str())
                })
            {
                return Err(diagnostic::invalid("trace-pins"));
            }
            if m.nodes
                .iter()
                .filter_map(|n| n.semantic_id.as_ref())
                .any(|s| !project.definitions.iter().any(|d| d.id().as_str() == s))
            {
                return Err(diagnostic::invalid("trace-semantic-id"));
            }
            joined.trace = Some(trace);
        }
        if let Some(a) = self.queries.value() {
            a.verify()?;
            let json: serde_json::Value = wire::decode(a.bytes.as_bytes())?;
            let queries = crate::query_model::QueryModelAttachment::from_value(&json)?;
            if queries.project_id().as_str() != pins.project
                || queries.ir_digest().as_str() != pins.ir_digest
                || pins.model_digest.value().map(String::as_str)
                    != Some(queries.model_ref().digest().as_str())
            {
                return Err(diagnostic::invalid("query-pins"));
            }
            for q in queries.queries() {
                let Some(Definition::Query(ir)) = project
                    .definitions
                    .iter()
                    .find(|d| d.id().as_str() == q.query.as_str())
                else {
                    return Err(diagnostic::invalid("query-reference"));
                };
                let Some(Definition::Entity(entity)) = project
                    .definitions
                    .iter()
                    .find(|d| d.id().as_str() == q.source.as_str())
                else {
                    return Err(diagnostic::invalid("query-source"));
                };
                if !ir.reads.iter().any(|r| r.as_str() == q.source.as_str()) {
                    return Err(diagnostic::invalid("query-read-edge"));
                }
                for field in query_fields(q) {
                    if !entity.fields.iter().any(|f| f.name.as_str() == field) {
                        return Err(diagnostic::invalid("query-field"));
                    }
                }
            }
            joined.queries = Some(queries);
        }
        if let Some(a) = self.transactions.value() {
            a.verify()?;
            let json: serde_json::Value = wire::decode(a.bytes.as_bytes())?;
            let tx = crate::transaction_concurrency::TransactionConcurrencyAttachment::from_value(
                &json,
            )?;
            if tx.project_id().as_str() != pins.project
                || tx.ir_digest().as_str() != pins.ir_digest
                || pins.model_digest.value().map(String::as_str)
                    != Some(tx.model_ref().digest().as_str())
                || tx.effect_graph_digest().map(|d| d.as_str()) != Some(effects.ir_digest())
            {
                return Err(diagnostic::invalid("transaction-pins"));
            }
            tx.validate_against_graph(effects)?;
            if tx
                .operations()
                .iter()
                .any(|op| !effects.knows_operation(op.operation_ref()))
            {
                return Err(diagnostic::invalid("transaction-operation"));
            }
            joined.transactions = Some(tx);
        }
        let mut obligations = BTreeSet::new();
        for r in &self.replicas {
            if !safe_token(&r.obligation)
                || !safe_token(&r.review_ref)
                || !obligations.insert(&r.obligation)
                || r.subjects.len() < 2
                || r.subjects.len() > 64
                || r.subjects.iter().collect::<BTreeSet<_>>().len() != r.subjects.len()
            {
                return Err(diagnostic::invalid("replica-shape"));
            }
            let mut shapes = vec![];
            joined.replica_subjects.extend(r.subjects.iter().cloned());
            for id in &r.subjects {
                let Some(d) = project
                    .definitions
                    .iter()
                    .find(|d| format!("{}:{}", kind(d), d.id().as_str()) == *id)
                else {
                    return Err(diagnostic::invalid("replica-id"));
                };
                if super::projection::fields(d).is_empty() {
                    return Err(diagnostic::invalid("replica-kind"));
                }
                let mut fields = super::projection::fields(d)
                    .iter()
                    .map(|f| {
                        (
                            f.name.as_str().to_owned(),
                            crate::diff::projection::type_expression(&f.r#type),
                            f.required,
                        )
                    })
                    .collect::<Vec<_>>();
                fields.sort();
                let identity = match d {
                    Definition::Entity(e) => {
                        e.identity.iter().map(|f| f.as_str()).collect::<Vec<_>>()
                    }
                    _ => vec![],
                };
                shapes.push((d.kind(), fields, identity));
            }
            if shapes.windows(2).any(|p| p[0] != p[1]) {
                for id in &r.subjects {
                    joined
                        .replica_divergences
                        .entry(id.clone())
                        .or_default()
                        .push(r.obligation.clone());
                }
            }
        }
        Ok(joined)
    }
}
pub fn kind(d: &Definition) -> &'static str {
    match d {
        Definition::Scalar(_) | Definition::Enum(_) | Definition::ValueObject(_) => "type",
        Definition::Entity(_) => "entity",
        Definition::Command(_) | Definition::Query(_) => "operation",
        Definition::Policy(_) => "policy",
        Definition::Event(_) => "event",
        Definition::Effect(_) => "effect",
        Definition::Endpoint(_) => "endpoint",
        Definition::Scenario(_) => "scenario",
        Definition::TargetBinding(_) => "target-binding",
    }
}
pub fn safe_token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 192
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-/#".contains(&b))
}
pub fn query_fields(q: &crate::query_model::query::QueryDecl) -> BTreeSet<String> {
    let mut refs: BTreeSet<_> = q
        .selection
        .iter()
        .flatten()
        .map(|f| f.field.as_str().to_owned())
        .chain(q.sort.iter().flatten().map(|f| f.field.as_str().to_owned()))
        .collect();
    if let Some(filter) = &q.filter {
        refs.extend(filter.leaves().iter().map(|f| f.field.as_str().to_owned()));
    }
    for i in &q.includes {
        if let Some(f) = i.path.first() {
            refs.insert(f.as_str().to_owned());
        }
    }
    refs
}
