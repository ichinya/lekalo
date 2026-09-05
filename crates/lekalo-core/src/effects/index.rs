//! The precomputed effect indexes (issue #14).
//!
//! Readers/writers and per-operation lookups answer from maps built once
//! during construction and extended on evidence attachment — never from a
//! rescan of the IR or the edge list. Positions point into the sorted
//! edge vectors, so answers come out in canonical order.

use std::collections::HashMap;

use super::edge::EffectEdge;
use super::identity::{OperationId, ResourceId};
use super::kind::EffectKind;

/// One combined index over the declared and detected edge vectors.
#[derive(Debug, Default)]
pub struct EffectIndex {
    /// Operation id to edge positions (declared positions first).
    by_operation: HashMap<OperationId, Vec<(Sector, u32)>>,
    /// Resource id to edge positions touching it (any scope).
    by_resource: HashMap<ResourceId, Vec<(Sector, u32)>>,
}

/// Which edge vector a position refers to.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Sector {
    /// The declared projection.
    Declared,
    /// The detected evidence projection.
    Detected,
}

impl EffectIndex {
    /// Build the index over both edge vectors (each already sorted).
    pub(crate) fn build(declared: &[EffectEdge], detected: &[EffectEdge]) -> Self {
        let mut index = Self::default();
        for (sector, edges) in [(Sector::Declared, declared), (Sector::Detected, detected)] {
            for (position, edge) in edges.iter().enumerate() {
                let Ok(position) = u32::try_from(position) else {
                    continue;
                };
                let key = edge.key();
                index
                    .by_operation
                    .entry(key.operation().clone())
                    .or_default()
                    .push((sector, position));
                index
                    .by_resource
                    .entry(key.subject().resource().clone())
                    .or_default()
                    .push((sector, position));
            }
        }
        index
    }

    /// Edge positions of one operation in canonical order.
    pub(crate) fn operation_edges<'a>(
        &self,
        operation: &OperationId,
        declared: &'a [EffectEdge],
        detected: &'a [EffectEdge],
    ) -> Vec<&'a EffectEdge> {
        self.resolve(self.by_operation.get(operation), declared, detected)
    }

    /// Edge positions touching one resource in canonical order.
    pub(crate) fn resource_edges<'a>(
        &self,
        resource: &ResourceId,
        declared: &'a [EffectEdge],
        detected: &'a [EffectEdge],
    ) -> Vec<&'a EffectEdge> {
        self.resolve(self.by_resource.get(resource), declared, detected)
    }

    fn resolve<'a>(
        &self,
        slots: Option<&Vec<(Sector, u32)>>,
        declared: &'a [EffectEdge],
        detected: &'a [EffectEdge],
    ) -> Vec<&'a EffectEdge> {
        let Some(slots) = slots else {
            return Vec::new();
        };
        slots
            .iter()
            .map(|(sector, position)| match sector {
                Sector::Declared => &declared[*position as usize],
                Sector::Detected => &detected[*position as usize],
            })
            .collect()
    }
}

/// Whether one edge kind is a read (the reverse-readers class).
pub(crate) const fn is_read(kind: EffectKind) -> bool {
    matches!(kind, EffectKind::Read)
}

/// Whether one edge kind is a write (the reverse-writers class): CRUD,
/// field writes, and cache mutations. Event, job, external, output, and
/// audit effects are neither reads nor writes.
pub(crate) const fn is_write(kind: EffectKind) -> bool {
    kind.is_mutation()
}
