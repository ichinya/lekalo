//! Pure semantic comparison of two same-family attachments (issue
//! #65).
//!
//! The comparison answers two questions per changed path: which layer
//! changed — **domain** (the target-neutral model), **storage** (a
//! target-namespaced projection), or **wire** (the contract envelope) —
//! and one closed compatibility class: **breaking** (a declared
//! guarantee was removed or narrowed), **non-breaking** (an addition
//! or a widening under the evolution policy), or **policy-change** (an
//! explicit owner decision, such as a delete behavior or a soft-delete
//! policy). Storage-layer paths carry a
//! visible data risk, so a migration obligation is never hidden inside
//! a class. A domain rename and a table rename are therefore two
//! different paths in two different layers: renaming the Model symbol
//! of an entity never implies renaming its table. Foreign projects are
//! the typed error set, never a guessed classification. Paths are
//! deterministic and byte-sorted.

use super::projection::{DataRisk, Projection, Table};
use super::{diagnostic, StorageProjectionAttachment};
use crate::diagnostics::DiagnosticSet;

/// The closed layer of one changed path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DiffLayer {
    /// The target-neutral domain model.
    Domain,
    /// A target-namespaced storage projection.
    Storage,
    /// The contract envelope.
    Wire,
}

impl DiffLayer {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Domain => "domain",
            Self::Storage => "storage",
            Self::Wire => "wire",
        }
    }
}

/// The closed compatibility class of one changed path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DiffClass {
    /// A required guarantee was removed or narrowed.
    Breaking,
    /// An addition or widening under the evolution policy.
    NonBreaking,
    /// An explicit owner decision with unchanged guarantees.
    PolicyChange,
}

impl DiffClass {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Breaking => "breaking",
            Self::NonBreaking => "non-breaking",
            Self::PolicyChange => "policy-change",
        }
    }
}

/// One changed path with its layer, class, and optional data risk.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DiffPath {
    path: String,
    layer: DiffLayer,
    class: DiffClass,
    risk: Option<DataRisk>,
}

impl DiffPath {
    /// The canonical path spelling.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The changed layer.
    pub const fn layer(&self) -> DiffLayer {
        self.layer
    }

    /// The closed compatibility class.
    pub const fn class(&self) -> DiffClass {
        self.class
    }

    /// The visible data risk; storage-layer obligations only.
    pub const fn risk(&self) -> Option<DataRisk> {
        self.risk
    }
}

/// The finished comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffResult {
    equal: bool,
    paths: Vec<DiffPath>,
}

impl DiffResult {
    /// Whether the two attachments are semantically equal.
    pub const fn equal(&self) -> bool {
        self.equal
    }

    /// The changed paths, byte-sorted.
    pub fn paths(&self) -> &[DiffPath] {
        &self.paths
    }
}

/// Compare two same-family attachments. Pure and read-only.
pub fn compare(
    base: &StorageProjectionAttachment,
    candidate: &StorageProjectionAttachment,
) -> Result<DiffResult, DiagnosticSet> {
    if base.project_id().as_str() != candidate.project_id().as_str() {
        return Err(diagnostic::diff_invalid("diff-project-mismatch"));
    }
    let mut paths: Vec<DiffPath> = Vec::new();
    if base.attachment_revision().as_str() != candidate.attachment_revision().as_str() {
        push_wire(&mut paths, "wire/attachmentRevision");
    }
    if base.model_ref().version().as_str() != candidate.model_ref().version().as_str()
        || base.model_ref().digest().as_str() != candidate.model_ref().digest().as_str()
    {
        push_wire(&mut paths, "wire/modelRef");
    }
    if base.ir_digest().as_str() != candidate.ir_digest().as_str() {
        push_wire(&mut paths, "wire/irRef");
    }
    compare_entities(base, candidate, &mut paths);
    compare_relations(base, candidate, &mut paths);
    compare_projections(base, candidate, &mut paths);
    paths.sort();
    Ok(DiffResult {
        equal: paths.is_empty(),
        paths,
    })
}

/// Record one wire-layer envelope change.
fn push_wire(paths: &mut Vec<DiffPath>, path: &str) {
    paths.push(DiffPath {
        path: path.to_owned(),
        layer: DiffLayer::Wire,
        class: DiffClass::PolicyChange,
        risk: None,
    });
}

/// Record one changed path.
fn push(
    paths: &mut Vec<DiffPath>,
    path: String,
    layer: DiffLayer,
    class: DiffClass,
    risk: Option<DataRisk>,
) {
    paths.push(DiffPath {
        path,
        layer,
        class,
        risk,
    });
}

/// Entity-layer comparison.
fn compare_entities(
    base: &StorageProjectionAttachment,
    candidate: &StorageProjectionAttachment,
    paths: &mut Vec<DiffPath>,
) {
    for entity in base.entities() {
        let key = entity.entity_key().as_str();
        let Some(other) = candidate.entity(entity.entity_key()) else {
            push(
                paths,
                format!("domain/entities/{key}"),
                DiffLayer::Domain,
                DiffClass::Breaking,
                None,
            );
            continue;
        };
        let prefix = format!("domain/entities/{key}");
        if entity.entity().as_str() != other.entity().as_str() {
            // A Model symbol rebind: exactly the rename case. The
            // stable entity key and every storage mapping stay put.
            push(
                paths,
                format!("{prefix}/entity"),
                DiffLayer::Domain,
                DiffClass::NonBreaking,
                None,
            );
        }
        if entity.external() != other.external() {
            push(
                paths,
                format!("{prefix}/external"),
                DiffLayer::Domain,
                DiffClass::Breaking,
                None,
            );
        }
        compare_visibility(
            paths,
            &format!("{prefix}/visibility"),
            entity.visibility(),
            other.visibility(),
        );
        if entity.aggregate_root() != other.aggregate_root()
            || entity.aggregate_owner().map(|owner| owner.as_str())
                != other.aggregate_owner().map(|owner| owner.as_str())
        {
            push(
                paths,
                format!("{prefix}/aggregate"),
                DiffLayer::Domain,
                DiffClass::PolicyChange,
                None,
            );
        }
        if entity.invariants() != other.invariants() {
            push(
                paths,
                format!("{prefix}/invariants"),
                DiffLayer::Domain,
                DiffClass::NonBreaking,
                None,
            );
        }
        if entity.state_spaces() != other.state_spaces() {
            push(
                paths,
                format!("{prefix}/stateSpaces"),
                DiffLayer::Domain,
                DiffClass::NonBreaking,
                None,
            );
        }
        if entity.description() != other.description() {
            push(
                paths,
                format!("{prefix}/description"),
                DiffLayer::Domain,
                DiffClass::NonBreaking,
                None,
            );
        }
        compare_fields(entity, other, &prefix, paths);
    }
    for entity in candidate.entities() {
        if base.entity(entity.entity_key()).is_none() {
            push(
                paths,
                format!("domain/entities/{}", entity.entity_key().as_str()),
                DiffLayer::Domain,
                DiffClass::NonBreaking,
                None,
            );
        }
    }
}

/// Field-level comparison under one entity.
fn compare_fields(
    base: &super::entity::DomainEntity,
    candidate: &super::entity::DomainEntity,
    prefix: &str,
    paths: &mut Vec<DiffPath>,
) {
    for field in base.fields() {
        let name = field.name().as_str();
        let Some(other) = candidate
            .fields()
            .iter()
            .find(|candidate| candidate.name().as_str() == name)
        else {
            push(
                paths,
                format!("{prefix}/fields/{name}"),
                DiffLayer::Domain,
                DiffClass::Breaking,
                None,
            );
            continue;
        };
        let path = format!("{prefix}/fields/{name}");
        if field.field_type() != other.field_type() {
            let class = if field.field_type().widens(other.field_type()) {
                DiffClass::NonBreaking
            } else {
                DiffClass::Breaking
            };
            push(paths, path.clone(), DiffLayer::Domain, class, None);
        }
        if field.required() != other.required() {
            push(
                paths,
                path.clone(),
                DiffLayer::Domain,
                DiffClass::Breaking,
                None,
            );
        }
        compare_visibility(paths, &path, field.visibility(), other.visibility());
    }
    for field in candidate.fields() {
        if !base.fields().iter().any(|base| base.name() == field.name()) {
            push(
                paths,
                format!("{prefix}/fields/{}", field.name().as_str()),
                DiffLayer::Domain,
                DiffClass::NonBreaking,
                None,
            );
        }
    }
}

/// Visibility narrowing is breaking; widening is not.
fn compare_visibility(
    paths: &mut Vec<DiffPath>,
    path: &str,
    base: super::entity::Visibility,
    candidate: super::entity::Visibility,
) {
    if base == candidate {
        return;
    }
    let class = if base.widens(candidate) {
        DiffClass::NonBreaking
    } else {
        DiffClass::Breaking
    };
    push(paths, path.to_owned(), DiffLayer::Domain, class, None);
}

/// Relation-layer comparison.
fn compare_relations(
    base: &StorageProjectionAttachment,
    candidate: &StorageProjectionAttachment,
    paths: &mut Vec<DiffPath>,
) {
    for relation in base.relations() {
        let id = relation.relation_id().as_str();
        let Some(other) = candidate
            .relations()
            .iter()
            .find(|candidate| candidate.relation_id().as_str() == id)
        else {
            push(
                paths,
                format!("domain/relations/{id}"),
                DiffLayer::Domain,
                DiffClass::Breaking,
                None,
            );
            continue;
        };
        let prefix = format!("domain/relations/{id}");
        if relation.kind() != other.kind() {
            push(
                paths,
                format!("{prefix}/kind"),
                DiffLayer::Domain,
                DiffClass::Breaking,
                None,
            );
        }
        if relation.owner() != other.owner() || relation.target() != other.target() {
            push(
                paths,
                format!("{prefix}/members"),
                DiffLayer::Domain,
                DiffClass::Breaking,
                None,
            );
        }
        if relation.min() != other.min() || relation.max() != other.max() {
            let narrowed = other.min() > relation.min() || other.max() < relation.max();
            let class = if narrowed {
                DiffClass::Breaking
            } else {
                DiffClass::NonBreaking
            };
            push(
                paths,
                format!("{prefix}/cardinality"),
                DiffLayer::Domain,
                class,
                None,
            );
        }
        if relation.delete_behavior() != other.delete_behavior() {
            push(
                paths,
                format!("{prefix}/deleteBehavior"),
                DiffLayer::Domain,
                DiffClass::PolicyChange,
                None,
            );
        }
        match (relation.foreign_key(), other.foreign_key()) {
            (None, None) => {}
            (Some(base_column), Some(candidate_column)) => {
                if base_column != candidate_column {
                    push(
                        paths,
                        format!("{prefix}/foreignKey"),
                        DiffLayer::Domain,
                        DiffClass::Breaking,
                        Some(DataRisk::Destructive),
                    );
                }
            }
            _ => {
                push(
                    paths,
                    format!("{prefix}/foreignKey"),
                    DiffLayer::Domain,
                    DiffClass::Breaking,
                    None,
                );
            }
        }
        if relation.scenarios() != other.scenarios()
            || relation.constraints() != other.constraints()
        {
            push(
                paths,
                format!("{prefix}/coverage"),
                DiffLayer::Domain,
                DiffClass::NonBreaking,
                None,
            );
        }
        if relation.description() != other.description() {
            push(
                paths,
                format!("{prefix}/description"),
                DiffLayer::Domain,
                DiffClass::NonBreaking,
                None,
            );
        }
    }
    for relation in candidate.relations() {
        if !base
            .relations()
            .iter()
            .any(|base| base.relation_id() == relation.relation_id())
        {
            push(
                paths,
                format!("domain/relations/{}", relation.relation_id().as_str()),
                DiffLayer::Domain,
                DiffClass::NonBreaking,
                None,
            );
        }
    }
}

/// Storage-layer comparison per namespace.
fn compare_projections(
    base: &StorageProjectionAttachment,
    candidate: &StorageProjectionAttachment,
    paths: &mut Vec<DiffPath>,
) {
    for projection in base.projections() {
        let namespace = projection.namespace().key();
        let Some(other) = candidate.projection(projection.namespace()) else {
            push(
                paths,
                format!("storage/{namespace}"),
                DiffLayer::Storage,
                DiffClass::Breaking,
                Some(DataRisk::Destructive),
            );
            continue;
        };
        let prefix = format!("storage/{namespace}");
        compare_projection(projection, other, &prefix, paths);
    }
    for projection in candidate.projections() {
        if base.projection(projection.namespace()).is_none() {
            push(
                paths,
                format!("storage/{}", projection.namespace().key()),
                DiffLayer::Storage,
                DiffClass::NonBreaking,
                None,
            );
        }
    }
}

/// One namespace's comparison.
fn compare_projection(
    base: &Projection,
    candidate: &Projection,
    prefix: &str,
    paths: &mut Vec<DiffPath>,
) {
    for table in base.tables() {
        let key = table.entity().as_str();
        let Some(other) = candidate
            .tables()
            .iter()
            .find(|candidate| candidate.entity() == table.entity())
        else {
            push(
                paths,
                format!("{prefix}/tables/{key}"),
                DiffLayer::Storage,
                DiffClass::Breaking,
                Some(DataRisk::Destructive),
            );
            continue;
        };
        compare_table(table, other, &format!("{prefix}/tables/{key}"), paths);
    }
    for table in candidate.tables() {
        if !base
            .tables()
            .iter()
            .any(|base| base.entity() == table.entity())
        {
            push(
                paths,
                format!("{prefix}/tables/{}", table.entity().as_str()),
                DiffLayer::Storage,
                DiffClass::NonBreaking,
                None,
            );
        }
    }
    compare_joins(base, candidate, prefix, paths);
    compare_polymorphics(base, candidate, prefix, paths);
    compare_migrations(base, candidate, prefix, paths);
}

/// One table's comparison.
fn compare_table(base: &Table, candidate: &Table, prefix: &str, paths: &mut Vec<DiffPath>) {
    if base.table() != candidate.table() {
        push(
            paths,
            format!("{prefix}/table"),
            DiffLayer::Storage,
            DiffClass::Breaking,
            Some(DataRisk::Destructive),
        );
    }
    if base.primary_key() != candidate.primary_key() {
        push(
            paths,
            format!("{prefix}/primaryKey"),
            DiffLayer::Storage,
            DiffClass::Breaking,
            Some(DataRisk::Destructive),
        );
    }
    if base.soft_delete() != candidate.soft_delete() {
        push(
            paths,
            format!("{prefix}/softDelete"),
            DiffLayer::Storage,
            DiffClass::PolicyChange,
            None,
        );
    }
    if base.tenant_key() != candidate.tenant_key() {
        push(
            paths,
            format!("{prefix}/tenantKey"),
            DiffLayer::Storage,
            DiffClass::PolicyChange,
            None,
        );
    }
    if base.timestamps() != candidate.timestamps() {
        push(
            paths,
            format!("{prefix}/timestamps"),
            DiffLayer::Storage,
            DiffClass::PolicyChange,
            None,
        );
    }
    // Technical columns.
    for column in base.technical_columns() {
        let name = column.name().as_str();
        let Some(other) = candidate
            .technical_columns()
            .iter()
            .find(|candidate| candidate.name().as_str() == name)
        else {
            push(
                paths,
                format!("{prefix}/technical/{name}"),
                DiffLayer::Storage,
                DiffClass::PolicyChange,
                Some(DataRisk::Destructive),
            );
            continue;
        };
        if column.storage_type() != other.storage_type() {
            push(
                paths,
                format!("{prefix}/technical/{name}"),
                DiffLayer::Storage,
                DiffClass::PolicyChange,
                Some(DataRisk::Destructive),
            );
        } else if column.nullable() != other.nullable() {
            let risk = if other.nullable() {
                None
            } else {
                Some(DataRisk::BackfillRequired)
            };
            push(
                paths,
                format!("{prefix}/technical/{name}"),
                DiffLayer::Storage,
                DiffClass::PolicyChange,
                risk,
            );
        } else if column.purpose() != other.purpose() {
            push(
                paths,
                format!("{prefix}/technical/{name}"),
                DiffLayer::Storage,
                DiffClass::PolicyChange,
                None,
            );
        }
    }
    for column in candidate.technical_columns() {
        if !base
            .technical_columns()
            .iter()
            .any(|base| base.name() == column.name())
        {
            push(
                paths,
                format!("{prefix}/technical/{}", column.name().as_str()),
                DiffLayer::Storage,
                DiffClass::NonBreaking,
                None,
            );
        }
    }
    // Generated columns.
    for column in base.generated_columns() {
        let name = column.name().as_str();
        let Some(other) = candidate
            .generated_columns()
            .iter()
            .find(|candidate| candidate.name().as_str() == name)
        else {
            push(
                paths,
                format!("{prefix}/generated/{name}"),
                DiffLayer::Storage,
                DiffClass::PolicyChange,
                Some(DataRisk::Destructive),
            );
            continue;
        };
        if column.kind() != other.kind() || column.storage_type() != other.storage_type() {
            push(
                paths,
                format!("{prefix}/generated/{name}"),
                DiffLayer::Storage,
                DiffClass::PolicyChange,
                Some(DataRisk::Destructive),
            );
        }
    }
    for column in candidate.generated_columns() {
        if !base
            .generated_columns()
            .iter()
            .any(|base| base.name() == column.name())
        {
            push(
                paths,
                format!("{prefix}/generated/{}", column.name().as_str()),
                DiffLayer::Storage,
                DiffClass::NonBreaking,
                None,
            );
        }
    }
    // Indexes: one aggregate path; a pure addition is non-breaking.
    if base.indexes() != candidate.indexes() {
        let pure_addition = candidate.indexes().len() > base.indexes().len()
            && candidate
                .indexes()
                .iter()
                .all(|candidate| base.indexes().contains(candidate));
        let class = if pure_addition {
            DiffClass::NonBreaking
        } else {
            DiffClass::PolicyChange
        };
        push(
            paths,
            format!("{prefix}/indexes"),
            DiffLayer::Storage,
            class,
            None,
        );
    }
}

/// Join comparison.
fn compare_joins(
    base: &Projection,
    candidate: &Projection,
    prefix: &str,
    paths: &mut Vec<DiffPath>,
) {
    for join in base.joins() {
        let Some(other) = candidate
            .joins()
            .iter()
            .find(|candidate| candidate.relation() == join.relation())
        else {
            push(
                paths,
                format!("{prefix}/joins/{}", join.relation().as_str()),
                DiffLayer::Storage,
                DiffClass::PolicyChange,
                Some(DataRisk::Destructive),
            );
            continue;
        };
        if join.table() != other.table()
            || join.columns() != other.columns()
            || join.unique_pair() != other.unique_pair()
        {
            push(
                paths,
                format!("{prefix}/joins/{}", join.relation().as_str()),
                DiffLayer::Storage,
                DiffClass::PolicyChange,
                None,
            );
        }
    }
    for join in candidate.joins() {
        if !base
            .joins()
            .iter()
            .any(|base| base.relation() == join.relation())
        {
            push(
                paths,
                format!("{prefix}/joins/{}", join.relation().as_str()),
                DiffLayer::Storage,
                DiffClass::NonBreaking,
                None,
            );
        }
    }
}

/// Polymorphic materialization comparison.
fn compare_polymorphics(
    base: &Projection,
    candidate: &Projection,
    prefix: &str,
    paths: &mut Vec<DiffPath>,
) {
    for materialization in base.polymorphics() {
        let Some(other) = candidate
            .polymorphics()
            .iter()
            .find(|candidate| candidate.relation() == materialization.relation())
        else {
            push(
                paths,
                format!(
                    "{prefix}/polymorphics/{}",
                    materialization.relation().as_str()
                ),
                DiffLayer::Storage,
                DiffClass::PolicyChange,
                None,
            );
            continue;
        };
        if materialization.key_column() != other.key_column()
            || materialization.type_column() != other.type_column()
        {
            push(
                paths,
                format!(
                    "{prefix}/polymorphics/{}",
                    materialization.relation().as_str()
                ),
                DiffLayer::Storage,
                DiffClass::PolicyChange,
                None,
            );
        }
    }
    for materialization in candidate.polymorphics() {
        if !base
            .polymorphics()
            .iter()
            .any(|base| base.relation() == materialization.relation())
        {
            push(
                paths,
                format!(
                    "{prefix}/polymorphics/{}",
                    materialization.relation().as_str()
                ),
                DiffLayer::Storage,
                DiffClass::PolicyChange,
                None,
            );
        }
    }
}

/// Migration-history comparison: bookkeeping with unchanged
/// guarantees; the records' own risks stay visible as data.
fn compare_migrations(
    base: &Projection,
    candidate: &Projection,
    prefix: &str,
    paths: &mut Vec<DiffPath>,
) {
    for migration in base.migration_history() {
        let Some(other) = candidate
            .migration_history()
            .iter()
            .find(|candidate| candidate.migration_id() == migration.migration_id())
        else {
            push(
                paths,
                format!("{prefix}/migrations/{}", migration.migration_id().as_str()),
                DiffLayer::Storage,
                DiffClass::PolicyChange,
                None,
            );
            continue;
        };
        if migration.tables() != other.tables() || migration.risk() != other.risk() {
            push(
                paths,
                format!("{prefix}/migrations/{}", migration.migration_id().as_str()),
                DiffLayer::Storage,
                DiffClass::PolicyChange,
                None,
            );
        }
    }
    for migration in candidate.migration_history() {
        if !base
            .migration_history()
            .iter()
            .any(|base| base.migration_id() == migration.migration_id())
        {
            push(
                paths,
                format!("{prefix}/migrations/{}", migration.migration_id().as_str()),
                DiffLayer::Storage,
                DiffClass::PolicyChange,
                None,
            );
        }
    }
}
