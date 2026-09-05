//! The closed effect-kind and action registries (issue #14).
//!
//! Every P0 effect kind and write action is a registry-backed identifier
//! with a fixed rank and an exact wire key. Kind/subject legality is
//! checked once, here, so an illegal combination (a field-scoped delete,
//! a write-field without a field) can never become an edge.

use std::fmt;

use super::identity::{ResourceKind, Subject};

/// The closed write-action vocabulary of field writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum WriteAction {
    /// Set the field to a value.
    Set,
    /// Clear the field.
    Clear,
    /// Append to the field.
    Append,
    /// Replace the field contents.
    Replace,
    /// Merge into the field contents.
    Merge,
}

impl WriteAction {
    /// The closed v1 action keys in registry rank order.
    pub(crate) const KEYS: [Self; 5] = [
        Self::Set,
        Self::Clear,
        Self::Append,
        Self::Replace,
        Self::Merge,
    ];

    /// The registry rank; the action sort key inside one subject.
    pub const fn rank(self) -> u8 {
        self as u8
    }

    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Clear => "clear",
            Self::Append => "append",
            Self::Replace => "replace",
            Self::Merge => "merge",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::KEYS
            .iter()
            .find(|candidate| candidate.key() == key)
            .copied()
    }
}

impl fmt::Display for WriteAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.key())
    }
}

/// The closed P0 effect-kind registry.
///
/// The payload each kind carries is typed: reads and writes name their
/// subject scope, `write-field` carries its closed action, and the
/// infrastructure kinds name the resource kind they may target. Entity
/// and field scope are distinct everywhere: an entity read does not imply
/// field reads, a field write does not imply entity replacement, and CRUD
/// is never a free-form write alias.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum EffectKind {
    /// Read a resource or one exact field.
    Read,
    /// Create a resource (entity-wide).
    Create,
    /// Update a resource, or one exact field when evidence identifies it.
    Update,
    /// Delete a resource (entity-wide).
    Delete,
    /// Write one exact field with a closed action.
    WriteField { action: WriteAction },
    /// Emit one event.
    EmitEvent,
    /// Enqueue one job.
    EnqueueJob,
    /// Call one external service.
    ExternalCall,
    /// Read one cache resource.
    CacheRead,
    /// Write one cache resource.
    CacheWrite,
    /// Invalidate one cache resource.
    CacheInvalidate,
    /// Publish one output.
    PublishOutput,
    /// Write one audit-log entry.
    AuditLog,
    /// Open or close a transaction boundary (a descriptive group only).
    TransactionBoundary,
}

/// The closed v1 kind keys in registry rank order (the wire spelling of
/// `write-field` folds its action into the sibling `action` field, so the
/// kind key list is action-free).
pub(crate) const EFFECT_KIND_KEYS: [&str; 14] = [
    "read",
    "create",
    "update",
    "delete",
    "write-field",
    "emit-event",
    "enqueue-job",
    "external-call",
    "cache-read",
    "cache-write",
    "cache-invalidate",
    "publish-output",
    "audit-log",
    "transaction-boundary",
];

impl EffectKind {
    /// The registry rank; the primary kind sort key.
    pub const fn rank(self) -> u8 {
        match self {
            Self::Read => 0,
            Self::Create => 1,
            Self::Update => 2,
            Self::Delete => 3,
            Self::WriteField { .. } => 4,
            Self::EmitEvent => 5,
            Self::EnqueueJob => 6,
            Self::ExternalCall => 7,
            Self::CacheRead => 8,
            Self::CacheWrite => 9,
            Self::CacheInvalidate => 10,
            Self::PublishOutput => 11,
            Self::AuditLog => 12,
            Self::TransactionBoundary => 13,
        }
    }

    /// The exact wire key (action-free; the action rides beside it).
    pub const fn key(self) -> &'static str {
        EFFECT_KIND_KEYS[self.rank() as usize]
    }

    /// The write action, for the one kind that carries one.
    pub const fn action(self) -> Option<WriteAction> {
        match self {
            Self::WriteField { action } => Some(action),
            _ => None,
        }
    }

    /// Whether reads and writes at entity and field level are distinct
    /// for this kind (they are distinct for every subject-carrying kind).
    pub const fn is_subject_scoped(self) -> bool {
        !matches!(self, Self::TransactionBoundary)
    }

    /// Whether this kind mutates state (the conflict matrix's write side).
    pub(crate) const fn is_mutation(self) -> bool {
        matches!(
            self,
            Self::Create
                | Self::Update
                | Self::Delete
                | Self::WriteField { .. }
                | Self::CacheWrite
                | Self::CacheInvalidate
        )
    }

    /// The resource kinds this kind may legally target.
    pub(crate) fn legal_resource_kinds(self) -> &'static [ResourceKind] {
        match self {
            Self::Read | Self::Update | Self::Create | Self::Delete => {
                &[ResourceKind::Canonical, ResourceKind::TargetResource]
            }
            Self::WriteField { .. } => &[ResourceKind::Canonical, ResourceKind::TargetResource],
            Self::EmitEvent => &[ResourceKind::Event],
            Self::EnqueueJob => &[ResourceKind::Job],
            Self::ExternalCall => &[ResourceKind::ExternalService],
            Self::CacheRead | Self::CacheWrite | Self::CacheInvalidate => &[ResourceKind::Cache],
            Self::PublishOutput => &[ResourceKind::Output],
            Self::AuditLog => &[ResourceKind::Audit],
            Self::TransactionBoundary => &[],
        }
    }
}

impl fmt::Display for EffectKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.key())
    }
}

/// Why one kind/subject/field combination is illegal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum KindViolation {
    /// The kind does not target this resource kind at all.
    ResourceKind {
        kind: &'static str,
        resource: &'static str,
    },
    /// The kind is entity-wide only but a field was supplied.
    UnexpectedField { kind: &'static str },
    /// `write-field` requires an exact field.
    MissingField { kind: &'static str },
    /// A transaction boundary carries no subject.
    UnexpectedSubject { kind: &'static str },
}

pub(crate) fn check_subject(kind: EffectKind, subject: &Subject) -> Result<(), KindViolation> {
    let resource_kind = subject.resource().kind();
    if !kind.legal_resource_kinds().contains(&resource_kind) {
        return Err(KindViolation::ResourceKind {
            resource: resource_kind.key(),
            kind: kind.key(),
        });
    }
    match kind {
        EffectKind::TransactionBoundary => Err(KindViolation::UnexpectedSubject {
            kind: "transaction-boundary",
        }),
        EffectKind::WriteField { .. } => {
            if subject.field().is_none() {
                Err(KindViolation::MissingField {
                    kind: "write-field",
                })
            } else {
                Ok(())
            }
        }
        EffectKind::Create | EffectKind::Delete => {
            if subject.field().is_some() {
                Err(KindViolation::UnexpectedField { kind: kind.key() })
            } else {
                Ok(())
            }
        }
        _ => Ok(()),
    }
}
