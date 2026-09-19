//! The PostgreSQL capability snapshot (issue #69): the #24 seam feed.
//!
//! [`build`] assembles the embedded engine answers into the exact
//! [`CapabilitySnapshot`] `transaction_concurrency::map_capabilities`
//! consumes: the closed transaction/isolation/lock/concurrency
//! answers with their documented bounds (`snapshot` isolation and
//! `lock.range` answer `partial` honestly — PostgreSQL implements
//! snapshot isolation through `REPEATABLE READ`, and there are no
//! key-range locks below `SERIALIZABLE`), plus the owner-published
//! version-matrix rows and the `postgres-sql` component provides.
//! Strict mapping blocks unsupported, unknown, and unapproved
//! partial support — the "planner guarantees checked on PostgreSQL"
//! acceptance criterion.

use crate::storage_projection::StorageProjectionAttachment;
use crate::transaction_concurrency::{CapabilitySnapshot, SnapshotSupport};

use super::super::StorageEngineAttachment;
use super::version_matrix;

/// The engine-local concurrency answers, byte-sorted by capability id.
pub const CONCURRENCY_ANSWERS: &[(&str, SnapshotSupport)] = &[
    ("concurrency.compare_and_set", SnapshotSupport::Full),
    ("concurrency.etag_if_match", SnapshotSupport::Partial),
    ("external.compensation", SnapshotSupport::Unsupported),
    ("idempotency.durable_key", SnapshotSupport::Partial),
    ("idempotency.replay", SnapshotSupport::Partial),
    ("invariant.unique_concurrent", SnapshotSupport::Full),
    ("isolation.none", SnapshotSupport::Full),
    ("isolation.read_committed", SnapshotSupport::Full),
    ("isolation.repeatable_read", SnapshotSupport::Full),
    ("isolation.serializable", SnapshotSupport::Full),
    ("isolation.snapshot", SnapshotSupport::Partial),
    ("lock.exclusive", SnapshotSupport::Full),
    ("lock.key", SnapshotSupport::Full),
    ("lock.range", SnapshotSupport::Partial),
    ("lock.shared", SnapshotSupport::Full),
    ("transaction.atomic_group", SnapshotSupport::Full),
    ("transaction.rollback", SnapshotSupport::Full),
];

/// The version-independent truths the `postgres-sql` component
/// provides on the storage axis (mirrored from the embedded component
/// registry so the snapshot is self-consistent without resolving a
/// profile document).
pub const COMPONENT_PROVIDES: &[(&str, SnapshotSupport)] = &[
    ("storage.explain", SnapshotSupport::Full),
    ("storage.introspection", SnapshotSupport::Full),
    ("storage.jsonb", SnapshotSupport::Full),
    ("storage.migrations", SnapshotSupport::Full),
    ("storage.partial_index", SnapshotSupport::Full),
    ("storage.pooling", SnapshotSupport::Full),
    ("storage.rls", SnapshotSupport::Full),
    ("storage.sequences", SnapshotSupport::Full),
    ("storage.sql", SnapshotSupport::Full),
    ("storage.transactions", SnapshotSupport::Full),
];

/// Build the capability snapshot for one engine profile bound to its
/// storage-projection attachment. The engine version must resolve to
/// one published matrix row (the caller has validated the profile, so
/// an unpublished major is a caller fault answered `Unknown`, which
/// strict mapping always blocks — unknown is never yes).
pub fn build(
    profile: &StorageEngineAttachment,
    _projection: &StorageProjectionAttachment,
) -> CapabilitySnapshot {
    let major = profile.engine_version().major();
    let entries = CONCURRENCY_ANSWERS
        .iter()
        .copied()
        .chain(COMPONENT_PROVIDES.iter().copied())
        .chain(
            version_matrix::CAPABILITY_IDS
                .iter()
                .filter_map(|id| version_matrix::answer(major, id).map(|support| (*id, support))),
        )
        .map(|(id, support)| (id.to_owned(), support));
    CapabilitySnapshot::new(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_engine::StorageEngineAttachment;
    use crate::transaction_concurrency::{map_capabilities, CapabilityProfile};

    fn profile_json(major: u32) -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": "lekalo/storage-engine/v0.4.0",
            "identity": "dev.lekalo.storage-engine@0.4.0",
            "attachmentRevision": "0.4.0",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "0.2.16",
                "digest": "sha256:0101010101010101010101010101010101010101010101010101010101010101"
            },
            "irRef": {
                "identity": "dev.lekalo.ir@0.2.16",
                "digest": "sha256:0202020202020202020202020202020202020202020202020202020202020202"
            },
            "projectionRef": "sha256:0303030303030303030303030303030303030303030303030303030303030303",
            "engine": "postgres",
            "engineVersion": format!("{major}.1.0"),
            "policies": {
                "identifierQuote": "always",
                "json": "jsonb",
                "enum": "check",
                "array": "native",
                "time": {"instant": "timestamptz", "local": "forbidden"},
                "pagination": {"offset": "allowed", "cursor": "keyset"}
            }
        })
    }

    fn profile(major: u32) -> StorageEngineAttachment {
        StorageEngineAttachment::from_value(&profile_json(major)).expect("valid profile")
    }

    fn empty_projection() -> StorageProjectionAttachment {
        // An attachment with no entities has no mapping obligations,
        // so the empty declaration parses and validates.
        let value = serde_json::json!({
            "schemaVersion": "lekalo/storage-projection/v0.4.0",
            "identity": "dev.lekalo.storage-projection@0.4.0",
            "attachmentRevision": "0.4.0",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "0.2.16",
                "digest": "sha256:0101010101010101010101010101010101010101010101010101010101010101"
            },
            "irRef": {
                "identity": "dev.lekalo.ir@0.2.16",
                "digest": "sha256:0202020202020202020202020202020202020202020202020202020202020202"
            },
            "entities": [],
            "relations": [],
            "projections": []
        });
        StorageProjectionAttachment::from_value(&value).expect("valid empty attachment")
    }

    #[test]
    fn concurrency_answers_are_honest_and_versioned() {
        let profile = profile(16);
        let projection = empty_projection();
        let snapshot = build(&profile, &projection);
        assert_eq!(
            snapshot.support_of("transaction.atomic_group"),
            SnapshotSupport::Full
        );
        assert_eq!(
            snapshot.support_of("isolation.serializable"),
            SnapshotSupport::Full
        );
        assert_eq!(
            snapshot.support_of("isolation.snapshot"),
            SnapshotSupport::Partial,
            "snapshot isolation stays honestly partial"
        );
        assert_eq!(
            snapshot.support_of("lock.range"),
            SnapshotSupport::Partial,
            "no key-range locks below serializable"
        );
        assert_eq!(
            snapshot.support_of("external.compensation"),
            SnapshotSupport::Unsupported
        );
        // The version matrix rides beside the engine answers.
        assert_eq!(
            snapshot.support_of("storage.postgres.partial_index"),
            SnapshotSupport::Full
        );
        assert_eq!(
            snapshot.support_of("storage.postgres.temporal_constraint"),
            SnapshotSupport::Unsupported,
            "18-only capability answers unsupported on 16"
        );
    }

    #[test]
    fn the_temporal_constraint_answer_flips_with_the_version() {
        let projection = empty_projection();
        let snapshot16 = build(&profile(16), &projection);
        let snapshot18 = build(&profile(18), &projection);
        assert_eq!(
            snapshot16.support_of("storage.postgres.temporal_constraint"),
            SnapshotSupport::Unsupported
        );
        assert_eq!(
            snapshot18.support_of("storage.postgres.temporal_constraint"),
            SnapshotSupport::Full
        );
    }

    #[test]
    fn an_unpublished_major_is_refused_and_strict_blocks_unknown() {
        use crate::scenario::id::NamespacedId;
        use crate::transaction_concurrency::precondition::{
            CapabilityId, CapabilityRequirement, RequirementLevel,
        };
        // An unpublished major refuses normalization outright: the pin
        // is unsupported, never clamped.
        let error =
            StorageEngineAttachment::from_value(&profile_json(99)).expect_err("unpublished major");
        assert_eq!(
            error.reason_ids().first().copied(),
            Some("storage-engine.version-unsupported")
        );
        // A caller-assembled snapshot without the matrix rows answers
        // unknown for the engine capabilities, and unknown is never
        // yes under the strict profile.
        let snapshot = CapabilitySnapshot::new(
            CONCURRENCY_ANSWERS
                .iter()
                .copied()
                .chain(COMPONENT_PROVIDES.iter().copied())
                .map(|(id, support)| (id.to_owned(), support)),
        );
        assert_eq!(
            snapshot.support_of("storage.postgres.temporal_constraint"),
            SnapshotSupport::Unknown
        );
        let requirements = [CapabilityRequirement {
            requirement_id: NamespacedId::parse("planner.req/compensation").expect("id"),
            capability: CapabilityId::ExternalCompensation,
            minimum: RequirementLevel::Full,
            reason: "owner-recorded reason".to_owned(),
        }];
        let decision = map_capabilities(&requirements, &snapshot, CapabilityProfile::Strict);
        assert!(decision.blocked(), "unsupported is never yes");
        let _ = empty_projection();
    }
}
