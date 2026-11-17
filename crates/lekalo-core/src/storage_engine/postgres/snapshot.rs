//! Capability answers belong to the external provider. A failed exchange
//! returns an empty snapshot, whose answers are Unknown and block strict mapping.
use super::super::{provider, StorageEngineAttachment};
use super::version_matrix;
use crate::storage_projection::StorageProjectionAttachment;
use crate::transaction_concurrency::CapabilitySnapshot;
#[cfg(test)]
use crate::transaction_concurrency::SnapshotSupport;
/// Build the external provider's capability projection.
pub fn build(
    profile: &StorageEngineAttachment,
    _projection: &StorageProjectionAttachment,
) -> CapabilitySnapshot {
    let entries = (|| {
        let value = provider::call("storage.matrix", serde_json::json!({}))?;
        let mut entries = version_matrix::answers(&value["concurrency"])?;
        entries.extend(version_matrix::answers(&value["provides"])?);
        if let Some(row) = value["rows"].as_array().and_then(|rows| {
            rows.iter()
                .find(|r| r["major"].as_u64() == Some(u64::from(profile.engine_version().major())))
        }) {
            entries.extend(version_matrix::answers(&row["answers"])?);
        }
        Ok::<_, crate::diagnostics::DiagnosticSet>(entries)
    })()
    .unwrap_or_default();
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
        let value = super::super::super::provider::call("storage.matrix", serde_json::json!({}))
            .expect("provider");
        let entries = version_matrix::answers(&value["concurrency"])
            .expect("answers")
            .into_iter()
            .chain(version_matrix::answers(&value["provides"]).expect("answers"));
        let snapshot = CapabilitySnapshot::new(entries);

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
