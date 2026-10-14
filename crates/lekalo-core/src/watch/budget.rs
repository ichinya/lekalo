//! Watch-specific admission and conservative retained-state charging.
//! Cache eviction is not semantic invalidation: mandatory ownership and
//! diagnostic partitions survive. These are not allocator/RSS measurements.
#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;
use super::{
    snapshot::{CaptureError, InputSnapshot},
    AcceptedState,
};
use crate::loader::frontends::{Node, Scalar, Value};
use std::mem::size_of;

pub(crate) const MAX_MODEL_BYTES: u64 = 1024 * 1024;
pub(crate) const MAX_MODULES: usize = 256;
pub(crate) const MAX_DEFINITIONS: usize = 2048;
pub(crate) const MAX_MODEL_NODES: usize = 32768;

pub(crate) fn admit(snapshot: &InputSnapshot) -> Result<(), CaptureError> {
    let bytes: u64 = snapshot
        .files
        .iter()
        .filter(|(path, _)| path.starts_with("lekalo/"))
        .map(|(_, record)| record.len)
        .sum();
    if bytes > MAX_MODEL_BYTES {
        return Err(CaptureError::Limit {
            limit: "model-bytes",
            count: bytes,
        });
    }
    let modules = snapshot
        .files
        .keys()
        .filter(|path| path.starts_with("lekalo/modules/") && path.ends_with("/module.yaml"))
        .count();
    if modules > MAX_MODULES {
        return Err(CaptureError::Limit {
            limit: "model-modules",
            count: modules as u64,
        });
    }
    Ok(())
}
fn nodes(node: &Node) -> usize {
    1 + match &node.value {
        Value::Scalar(_) => 0,
        Value::Seq(rows) => rows.iter().map(nodes).sum(),
        Value::Map(rows) => rows.iter().map(|r| nodes(&r.value)).sum(),
    }
}
pub(crate) fn admit_model(
    model: &crate::loader::NormalizedModel,
) -> Result<(), crate::DomainResult> {
    let count: usize = model
        .project
        .iter()
        .chain(model.modules.iter())
        .chain(model.definitions.iter())
        .map(|r| nodes(&r.node))
        .sum();
    let failure = if model.definitions.len() > MAX_DEFINITIONS {
        Some(("model-definitions", model.definitions.len()))
    } else if count > MAX_MODEL_NODES {
        Some(("model-nodes", count))
    } else {
        None
    };
    match failure {
        Some((limit, count)) => Err(CaptureError::Limit {
            limit,
            count: count as u64,
        }
        .domain_result()),
        None => Ok(()),
    }
}
/// Dynamic AST allocation, including container capacity, not just live rows.
pub(crate) fn node_heap(node: &Node) -> usize {
    match &node.value {
        Value::Scalar(Scalar::Str(text)) => text.capacity(),
        Value::Scalar(_) => 0,
        Value::Seq(rows) => {
            rows.capacity() * size_of::<Node>() + rows.iter().map(node_heap).sum::<usize>()
        }
        Value::Map(rows) => {
            rows.capacity() * size_of::<crate::loader::frontends::MapEntry>()
                + rows
                    .iter()
                    .map(|r| r.key.capacity() + node_heap(&r.value))
                    .sum::<usize>()
        }
    }
}
impl AcceptedState {
    pub(crate) fn charged_bytes(&self) -> usize {
        let metadata = serde_json::to_vec(&(
            &self.revision,
            &self.files,
            &self.generated,
            &self.module_index.owned_paths,
            &self.module_index.imports,
            &self.symbols_by_path,
            &self.fingerprints,
            &self.partitions,
            &self.global_partition,
            &self.global_fingerprint,
            &self.drift_fingerprint,
            &self.drift,
        ))
        .map_or(usize::MAX / 4, |bytes| bytes.len());
        let rows = self.files.len()
            + self.generated.len()
            + self.module_index.owned_paths.len()
            + self.module_index.imports.len()
            + self.symbols_by_path.len()
            + self.fingerprints.len()
            + self.partitions.len()
            + self
                .module_index
                .owned_paths
                .values()
                .map(|r| r.len())
                .sum::<usize>()
            + self
                .module_index
                .imports
                .values()
                .map(|r| r.len())
                .sum::<usize>();
        let diagnostics = self.global_partition.capacity()
            + self
                .partitions
                .values()
                .map(|r| r.capacity())
                .sum::<usize>();
        let documents = self
            .documents
            .iter()
            .map(|(key, (digest, document))| {
                256 + key.capacity()
                    + digest.capacity()
                    + document.path.capacity()
                    + document.version.capacity()
                    + document.definitions.capacity()
                        * size_of::<crate::loader::project_docs::Definition>()
                    + document
                        .definitions
                        .iter()
                        .map(|r| r.id.capacity() + node_heap(&r.node))
                        .sum::<usize>()
            })
            .sum::<usize>();
        size_of::<Self>()
            .saturating_add(metadata.saturating_mul(4))
            .saturating_add(rows.saturating_mul(256))
            .saturating_add(diagnostics.saturating_mul(size_of::<crate::diagnostics::Diagnostic>()))
            .saturating_add(documents)
            .saturating_add(self.fragments.charged_bytes())
            .saturating_add(self.graph_fragments.charged_bytes())
    }
    pub(crate) fn fit_budget(&mut self, maximum: usize) -> Result<(), usize> {
        if self.charged_bytes() <= maximum {
            return Ok(());
        }
        self.graph_fragments = Default::default();
        if self.charged_bytes() <= maximum {
            return Ok(());
        }
        self.fragments = Default::default();
        if self.charged_bytes() <= maximum {
            return Ok(());
        }
        self.documents.clear();
        let remaining = self.charged_bytes();
        if remaining <= maximum {
            Ok(())
        } else {
            Err(remaining)
        }
    }
}
