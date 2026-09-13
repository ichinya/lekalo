//! Semantic validation of the storage-projection attachment (issue
//! #65).
//!
//! The closed semantic rules over one fully parsed attachment:
//! reference resolution across entities, relations, and projections;
//! aggregate coherence (roots, owners, acyclicity); relation
//! kind/cardinality/delete-behavior coherence with mandatory scenario
//! or constraint coverage; external-entity storage exclusion; and the
//! projection-to-domain mapping (total mapping of local entities,
//! column collision freedom, resolvable keys, complete join and
//! polymorphic materializations, and resolvable migration history).
//! Every violation is one registered diagnostic over the typed rule
//! set with no partial result. Pure and read-only.

use super::diagnostic::{self, DOMAIN_INVALID, PROJECTION_INVALID, RELATION_INVALID};
use super::entity::DomainEntity;
use super::id::{EntityKey, StorageName};
use super::projection::Projection;
use super::relation::{DeleteBehavior, Relation, RelationKind};
use super::StorageProjectionAttachment;
use crate::diagnostics::DiagnosticSet;
use std::collections::HashSet;

/// The semantic self-check over one assembled attachment.
pub(crate) fn semantic_self_check(
    attachment: &StorageProjectionAttachment,
) -> Result<(), DiagnosticSet> {
    check_entities(attachment)?;
    check_relations(attachment)?;
    check_projections(attachment)?;
    Ok(())
}

/// Entity-level rules: aggregate coherence and external aggregation
/// exclusion. Uniqueness is enforced at normalization time.
fn check_entities(attachment: &StorageProjectionAttachment) -> Result<(), DiagnosticSet> {
    for entity in attachment.entities() {
        let subject = entity.entity_key().as_str();
        if entity.aggregate_root() && entity.aggregate_owner().is_some() {
            return Err(diagnostic::rule_invalid(
                DOMAIN_INVALID,
                "aggregate-both",
                Some(subject),
            ));
        }
        if entity.external() && (entity.aggregate_root() || entity.aggregate_owner().is_some()) {
            return Err(diagnostic::rule_invalid(
                DOMAIN_INVALID,
                "external-aggregate",
                Some(subject),
            ));
        }
        if let Some(owner) = entity.aggregate_owner() {
            let root = attachment.entity(owner).ok_or_else(|| {
                diagnostic::rule_invalid(DOMAIN_INVALID, "aggregate-owner-missing", Some(subject))
            })?;
            if !root.aggregate_root() {
                return Err(diagnostic::rule_invalid(
                    DOMAIN_INVALID,
                    "aggregate-owner-not-root",
                    Some(subject),
                ));
            }
        }
    }
    Ok(())
}

/// Relation-level rules: reference resolution, kind coherence,
/// cardinality coherence, explicit delete behavior coherence, and
/// scenario or constraint coverage.
fn check_relations(attachment: &StorageProjectionAttachment) -> Result<(), DiagnosticSet> {
    for relation in attachment.relations() {
        let subject = relation.relation_id().as_str();
        if relation.min() > relation.max() {
            return Err(diagnostic::rule_invalid(
                RELATION_INVALID,
                "min-above-max",
                Some(subject),
            ));
        }
        let owner = require_entity(attachment, relation.owner(), subject)?;
        let target = require_entity(attachment, relation.target(), subject)?;
        if owner.external() && target.external() {
            return Err(diagnostic::rule_invalid(
                RELATION_INVALID,
                "external-both",
                Some(subject),
            ));
        }
        let has_coverage = !relation.scenarios().is_empty() || !relation.constraints().is_empty();
        if !has_coverage {
            return Err(diagnostic::rule_invalid(
                RELATION_INVALID,
                "coverage-missing",
                Some(subject),
            ));
        }
        let foreign_key_declared = relation.foreign_key().is_some();
        match relation.kind() {
            RelationKind::OneToOne => {
                if relation.max() != 1 {
                    return Err(kind_invalid(subject, "one-to-one-max"));
                }
                check_foreign_key_kind(relation, subject)?;
                check_detach_minimum(relation, subject)?;
            }
            RelationKind::OneToMany => {
                if relation.max() < 2 {
                    return Err(kind_invalid(subject, "one-to-many-max"));
                }
                check_foreign_key_kind(relation, subject)?;
                check_detach_minimum(relation, subject)?;
            }
            RelationKind::ManyToMany => {
                if foreign_key_declared {
                    return Err(kind_invalid(subject, "foreign-key-forbidden"));
                }
            }
            RelationKind::ExternalReference => {
                if !target.external() {
                    return Err(kind_invalid(subject, "external-target-required"));
                }
                if relation.delete_behavior() != DeleteBehavior::Detach {
                    return Err(kind_invalid(subject, "external-delete-behavior"));
                }
                if foreign_key_declared {
                    return Err(kind_invalid(subject, "foreign-key-forbidden"));
                }
            }
            RelationKind::AggregateChild => {
                if relation.delete_behavior() == DeleteBehavior::Detach {
                    return Err(kind_invalid(subject, "aggregate-child-delete-behavior"));
                }
                if target.aggregate_owner().map(|owner| owner.as_str())
                    != Some(owner.entity_key().as_str())
                {
                    return Err(kind_invalid(subject, "aggregate-child-ownership"));
                }
                check_foreign_key_kind(relation, subject)?;
                check_detach_minimum(relation, subject)?;
            }
            RelationKind::OptionalReference => {
                if relation.min() != 0 {
                    return Err(kind_invalid(subject, "optional-reference-min"));
                }
                if relation.delete_behavior() == DeleteBehavior::Cascade {
                    return Err(kind_invalid(subject, "optional-reference-delete"));
                }
                check_foreign_key_kind(relation, subject)?;
            }
            RelationKind::Polymorphic => {
                if foreign_key_declared {
                    return Err(kind_invalid(subject, "foreign-key-forbidden"));
                }
                check_detach_minimum(relation, subject)?;
            }
        }
    }
    Ok(())
}

/// One kind-coherence refusal.
fn kind_invalid(subject: &str, detail: &'static str) -> DiagnosticSet {
    diagnostic::rule_invalid(RELATION_INVALID, detail, Some(subject))
}

/// The foreign-key presence rule for the kinds that declare one.
fn check_foreign_key_kind(relation: &Relation, subject: &str) -> Result<(), DiagnosticSet> {
    if relation.foreign_key().is_some() {
        Ok(())
    } else {
        Err(kind_invalid(subject, "foreign-key-required"))
    }
}

/// Detach severs the relation; a mandatory minimum cannot detach.
fn check_detach_minimum(relation: &Relation, subject: &str) -> Result<(), DiagnosticSet> {
    if relation.delete_behavior() == DeleteBehavior::Detach && relation.min() != 0 {
        Err(kind_invalid(subject, "detach-minimum"))
    } else {
        Ok(())
    }
}

/// Projection-level rules: namespace identity, total mapping of local
/// entities with external exclusion, table-name uniqueness, column
/// collision freedom, resolvable keys and indexes, complete join and
/// polymorphic materializations, and resolvable migration history.
fn check_projections(attachment: &StorageProjectionAttachment) -> Result<(), DiagnosticSet> {
    for projection in attachment.projections() {
        check_mapping(attachment, projection)?;
        check_tables(attachment, projection)?;
        check_joins(attachment, projection)?;
        check_join_coverage(attachment, projection)?;
        check_polymorphics(attachment, projection)?;
        check_migrations(projection)?;
    }
    Ok(())
}

/// Every local entity is mapped exactly once and no external entity is
/// mapped; table names are unique across tables and joins.
fn check_mapping(
    attachment: &StorageProjectionAttachment,
    projection: &Projection,
) -> Result<(), DiagnosticSet> {
    let mapped: HashSet<&str> = projection
        .tables()
        .iter()
        .map(|table| table.entity().as_str())
        .collect();
    if mapped.len() != projection.tables().len() {
        return Err(projection_invalid("duplicate-table-entity"));
    }
    for entity in attachment.entities() {
        let is_mapped = mapped.contains(entity.entity_key().as_str());
        if entity.external() && is_mapped {
            return Err(projection_invalid_subject(
                "external-mapped",
                entity.entity_key().as_str(),
            ));
        }
        if !entity.external() && !is_mapped {
            return Err(projection_invalid_subject(
                "entity-unmapped",
                entity.entity_key().as_str(),
            ));
        }
    }
    let mut names: Vec<&str> = projection
        .tables()
        .iter()
        .map(|table| table.table().as_str())
        .collect();
    names.extend(projection.joins().iter().map(|join| join.table().as_str()));
    let sorted: Vec<&str> = {
        let mut sorted = names.clone();
        sorted.sort();
        sorted
    };
    if sorted.windows(2).any(|window| window[0] == window[1]) {
        return Err(projection_invalid("duplicate-table-name"));
    }
    Ok(())
}

/// Table-level rules: primary-key and index resolution and column
/// collision freedom over the derived and declared column set.
fn check_tables(
    attachment: &StorageProjectionAttachment,
    projection: &Projection,
) -> Result<(), DiagnosticSet> {
    for table in projection.tables() {
        let subject = table.entity().as_str();
        let entity = require_entity(attachment, table.entity(), table.entity().as_str())?;
        // The primary key and index lists reference columns; only columns
        // that exist by declaration or derivation can collide.
        let mut declared: Vec<&StorageName> = Vec::new();
        for technical in table.technical_columns() {
            declared.push(technical.name());
        }
        for generated in table.generated_columns() {
            declared.push(generated.name());
        }
        if let Some(column) = table.soft_delete() {
            declared.push(column);
        }
        if let Some((column, _)) = table.tenant_key() {
            declared.push(column);
        }
        if let Some((created_at, updated_at)) = table.timestamps() {
            declared.push(created_at);
            declared.push(updated_at);
        }
        let field_names: Vec<&str> = entity
            .fields()
            .iter()
            .map(|field| field.name().as_str())
            .collect();
        let mut merged: Vec<&str> = field_names.clone();
        for column in &declared {
            merged.push(column.as_str());
        }
        // Foreign-key and polymorphic columns join the merged set:
        // primary keys and indexes may name them only through the
        // declared and derived columns resolved here.
        for relation in attachment.relations() {
            if let Some(column) = relation.foreign_key() {
                if (relation.kind().target_foreign_key()
                    && relation.target().as_str() == table.entity.as_str())
                    || (relation.kind().owner_foreign_key()
                        && relation.owner().as_str() == table.entity.as_str())
                {
                    merged.push(column.as_str());
                }
            }
        }
        for materialization in projection.polymorphics() {
            let Some(relation) = attachment
                .relations()
                .iter()
                .find(|relation| relation.relation_id() == materialization.relation())
            else {
                continue;
            };
            if relation.kind() == RelationKind::Polymorphic && relation.owner().as_str() == subject
            {
                merged.push(materialization.key_column().as_str());
                merged.push(materialization.type_column().as_str());
            }
        }
        merged.sort();
        if merged.windows(2).any(|window| window[0] == window[1]) {
            return Err(projection_invalid_subject("column-collision", subject));
        }
        for column in table.primary_key() {
            if !merged.contains(&column.as_str()) {
                return Err(projection_invalid_subject("unknown-primary-key", subject));
            }
        }
        if table.primary_key().len() > 1 {
            // Composite keys are legal declarations, but the derived
            // foreign-key, join, and polymorphic machinery refuses
            // them explicitly instead of guessing a key column.
            let referenced = attachment.relations().iter().any(|relation| {
                relation.owner().as_str() == subject || relation.target().as_str() == subject
            });
            if referenced {
                return Err(projection_invalid_subject(
                    "composite-key-derivation",
                    subject,
                ));
            }
        }
        for index in table.indexes() {
            for column in index.columns() {
                if !merged.contains(&column.as_str()) {
                    return Err(projection_invalid_subject("unknown-index-column", subject));
                }
            }
        }
    }
    Ok(())
}

/// Join-level rules: each join materializes one many-to-many relation
/// of mapped entities exactly once with resolvable key columns.
fn check_joins(
    attachment: &StorageProjectionAttachment,
    projection: &Projection,
) -> Result<(), DiagnosticSet> {
    for join in projection.joins() {
        let Some(relation) = attachment
            .relations()
            .iter()
            .find(|relation| relation.relation_id() == join.relation())
        else {
            return Err(projection_invalid_subject(
                "join-unknown-relation",
                join.relation().as_str(),
            ));
        };
        if relation.kind() != RelationKind::ManyToMany {
            return Err(projection_invalid_subject(
                "join-kind",
                join.relation().as_str(),
            ));
        }
        for side in [relation.owner(), relation.target()] {
            if !projection
                .tables()
                .iter()
                .any(|table| table.entity() == side)
            {
                return Err(projection_invalid_subject(
                    "join-entity-unmapped",
                    side.as_str(),
                ));
            }
            let table = projection
                .tables()
                .iter()
                .find(|table| table.entity() == side)
                .expect("mapped table");
            if table.primary_key().len() != 1 {
                return Err(projection_invalid_subject(
                    "composite-key-derivation",
                    side.as_str(),
                ));
            }
        }
        if join.columns[0] == join.columns[1] {
            return Err(projection_invalid_subject(
                "join-columns",
                join.relation().as_str(),
            ));
        }
    }
    Ok(())
}

/// Every many-to-many relation between two mapped entities is
/// materialized exactly once in this namespace.
fn check_join_coverage(
    attachment: &StorageProjectionAttachment,
    projection: &Projection,
) -> Result<(), DiagnosticSet> {
    for relation in attachment.relations() {
        if relation.kind() != RelationKind::ManyToMany {
            continue;
        }
        let both_mapped = projection
            .tables()
            .iter()
            .any(|table| table.entity() == relation.owner())
            && projection
                .tables()
                .iter()
                .any(|table| table.entity() == relation.target());
        if !both_mapped {
            continue;
        }
        let count = projection
            .joins()
            .iter()
            .filter(|join| join.relation() == relation.relation_id())
            .count();
        if count != 1 {
            return Err(projection_invalid_subject(
                "join-materialization-missing",
                relation.relation_id().as_str(),
            ));
        }
    }
    Ok(())
}

/// Polymorphic-level rules: exactly one materialization per polymorphic
/// relation of a mapped owner, with distinct declared columns.
fn check_polymorphics(
    attachment: &StorageProjectionAttachment,
    projection: &Projection,
) -> Result<(), DiagnosticSet> {
    for materialization in projection.polymorphics() {
        let Some(relation) = attachment
            .relations()
            .iter()
            .find(|relation| relation.relation_id() == materialization.relation())
        else {
            return Err(projection_invalid_subject(
                "polymorphic-unknown-relation",
                materialization.relation().as_str(),
            ));
        };
        if relation.kind() != RelationKind::Polymorphic {
            return Err(projection_invalid_subject(
                "polymorphic-kind",
                materialization.relation().as_str(),
            ));
        }
        if !projection
            .tables()
            .iter()
            .any(|table| table.entity() == relation.owner())
        {
            return Err(projection_invalid_subject(
                "polymorphic-owner-unmapped",
                relation.owner().as_str(),
            ));
        }
        if materialization.key_column() == materialization.type_column() {
            return Err(projection_invalid_subject(
                "polymorphic-columns",
                materialization.relation().as_str(),
            ));
        }
    }
    for relation in attachment.relations() {
        if relation.kind() != RelationKind::Polymorphic {
            continue;
        }
        if !projection
            .tables()
            .iter()
            .any(|table| table.entity() == relation.owner())
        {
            continue;
        }
        let count = projection
            .polymorphics()
            .iter()
            .filter(|materialization| materialization.relation() == relation.relation_id())
            .count();
        if count != 1 {
            return Err(projection_invalid_subject(
                "polymorphic-materialization-missing",
                relation.relation_id().as_str(),
            ));
        }
    }
    Ok(())
}

/// Migration-history rules: every named table exists in this
/// projection. Identifier uniqueness is enforced at normalization.
fn check_migrations(projection: &Projection) -> Result<(), DiagnosticSet> {
    for migration in projection.migration_history() {
        for table in migration.tables() {
            if !projection
                .tables()
                .iter()
                .any(|declared| declared.table() == table)
                && !projection.joins().iter().any(|join| join.table() == table)
            {
                return Err(projection_invalid_subject(
                    "migration-unknown-table",
                    table.as_str(),
                ));
            }
        }
    }
    Ok(())
}

/// Resolve one entity or fail.
fn require_entity<'a>(
    attachment: &'a StorageProjectionAttachment,
    key: &EntityKey,
    subject: &str,
) -> Result<&'a DomainEntity, DiagnosticSet> {
    attachment
        .entity(key)
        .ok_or_else(|| diagnostic::rule_invalid(RELATION_INVALID, "entity-absent", Some(subject)))
}

/// One projection-structure refusal.
fn projection_invalid(detail: &'static str) -> DiagnosticSet {
    diagnostic::rule_invalid(PROJECTION_INVALID, detail, None)
}

/// One projection-structure refusal with a bounded subject echo.
fn projection_invalid_subject(detail: &'static str, subject: &str) -> DiagnosticSet {
    diagnostic::rule_invalid(PROJECTION_INVALID, detail, Some(subject))
}
