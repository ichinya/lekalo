//! Storage projection derivation and the public DTO projection
//! (issue #65).
//!
//! [`derive`] is the published pure function from one validated
//! attachment to the target-namespaced storage projection: every
//! column is either a declared domain field mapped through the
//! namespace type table, a foreign key derived from one relation's
//! explicit declaration, an explicit storage fact, or an explicit
//! polymorphic materialization. Nothing is invented: a derivation that
//! cannot name a column or resolve a primary key refuses with a
//! registered diagnostic. The same domain model derives the PostgreSQL
//! and Laravel projections, which is what makes them two renderings of
//! one source instead of two models.
//!
//! [`public_fields`] derives the public DTO members of one entity from
//! the domain declarations only. Storage-only technical columns live
//! in the projection layer and can never enter this result, which is
//! the structural guarantee that a storage field does not leak into
//! the public surface.

use crate::diagnostics::DiagnosticSet;
use crate::scenario::id::FieldName;

use super::entity::{DomainType, Visibility};
use super::id::{EntityKey, StorageName};
use super::projection::{Namespace, Projection};
use super::relation::{DeleteBehavior, RelationKind};
use super::{diagnostic, StorageProjectionAttachment};

/// Why one derived column exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ColumnOrigin {
    /// A declared domain field.
    Field,
    /// A relation foreign key.
    ForeignKey,
    /// A declared technical column.
    Technical,
    /// A declared generated column.
    Generated,
    /// The soft-delete policy column.
    SoftDelete,
    /// The tenant partition key.
    TenantKey,
    /// The audit creation timestamp.
    CreatedAt,
    /// The audit update timestamp.
    UpdatedAt,
    /// A polymorphic target key.
    DiscriminatorKey,
    /// A polymorphic discriminator type.
    DiscriminatorType,
    /// A join-table key column.
    JoinKey,
}

impl ColumnOrigin {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Field => "field",
            Self::ForeignKey => "foreign_key",
            Self::Technical => "technical",
            Self::Generated => "generated",
            Self::SoftDelete => "soft_delete",
            Self::TenantKey => "tenant_key",
            Self::CreatedAt => "created_at",
            Self::UpdatedAt => "updated_at",
            Self::DiscriminatorKey => "discriminator_key",
            Self::DiscriminatorType => "discriminator_type",
            Self::JoinKey => "join_key",
        }
    }
}

/// The closed foreign-key delete action.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum OnDelete {
    /// Delete the dependent rows.
    Cascade,
    /// Forbid the delete.
    Restrict,
    /// Null the reference (an explicit nullable foreign key only).
    SetNull,
}

impl OnDelete {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Cascade => "CASCADE",
            Self::Restrict => "RESTRICT",
            Self::SetNull => "SET NULL",
        }
    }

    /// The action for one relation kind and declared behavior.
    pub(crate) const fn of(behavior: DeleteBehavior) -> Self {
        match behavior {
            DeleteBehavior::Cascade => Self::Cascade,
            DeleteBehavior::Restrict => Self::Restrict,
            DeleteBehavior::Detach => Self::SetNull,
        }
    }
}

/// One derived column.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedColumn {
    pub(crate) name: StorageName,
    pub(crate) storage_type: String,
    pub(crate) nullable: bool,
    pub(crate) origin: ColumnOrigin,
    pub(crate) visibility: Visibility,
}

impl DerivedColumn {
    /// The column name.
    pub fn name(&self) -> &StorageName {
        &self.name
    }

    /// The derived storage type token.
    pub fn storage_type(&self) -> &str {
        &self.storage_type
    }

    /// Whether the column accepts null.
    pub const fn nullable(&self) -> bool {
        self.nullable
    }

    /// Why the column exists.
    pub const fn origin(&self) -> ColumnOrigin {
        self.origin
    }

    /// The domain visibility of the column's content. Technical and
    /// policy columns are private by construction.
    pub const fn visibility(&self) -> Visibility {
        self.visibility
    }
}

/// One derived foreign key.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DerivedForeignKey {
    pub(crate) column: StorageName,
    pub(crate) references_table: StorageName,
    pub(crate) on_delete: OnDelete,
}

impl DerivedForeignKey {
    /// The local foreign-key column.
    pub fn column(&self) -> &StorageName {
        &self.column
    }

    /// The referenced table.
    pub fn references_table(&self) -> &StorageName {
        &self.references_table
    }

    /// The declared delete action.
    pub const fn on_delete(&self) -> OnDelete {
        self.on_delete
    }
}

/// One derived polymorphic materialization on its owner table.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DerivedPolymorphic {
    pub(crate) relation: crate::scenario::id::SemanticId,
    pub(crate) key_column: StorageName,
    pub(crate) type_column: StorageName,
}

impl DerivedPolymorphic {
    /// The materialized relation.
    pub fn relation(&self) -> &crate::scenario::id::SemanticId {
        &self.relation
    }

    /// The target-key column.
    pub fn key_column(&self) -> &StorageName {
        &self.key_column
    }

    /// The discriminator column.
    pub fn type_column(&self) -> &StorageName {
        &self.type_column
    }
}

/// One derived table: every column of one entity's storage home.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedTable {
    pub(crate) entity: EntityKey,
    pub(crate) name: StorageName,
    pub(crate) columns: Vec<DerivedColumn>,
    pub(crate) primary_key: Vec<StorageName>,
    pub(crate) foreign_keys: Vec<DerivedForeignKey>,
    pub(crate) indexes: Vec<super::projection::Index>,
    pub(crate) polymorphics: Vec<DerivedPolymorphic>,
}

impl DerivedTable {
    /// The mapped entity key.
    pub fn entity(&self) -> &EntityKey {
        &self.entity
    }

    /// The table name.
    pub fn table(&self) -> &StorageName {
        &self.name
    }

    /// Every derived column, byte-sorted by name.
    pub fn columns(&self) -> &[DerivedColumn] {
        &self.columns
    }

    /// The declared primary-key columns, in declared order.
    pub fn primary_key(&self) -> &[StorageName] {
        &self.primary_key
    }

    /// The derived foreign keys, byte-sorted by column.
    pub fn foreign_keys(&self) -> &[DerivedForeignKey] {
        &self.foreign_keys
    }

    /// The declared and derived indexes, canonical order.
    pub fn indexes(&self) -> &[super::projection::Index] {
        &self.indexes
    }

    /// The derived polymorphic materializations, canonical order.
    pub fn polymorphics(&self) -> &[DerivedPolymorphic] {
        &self.polymorphics
    }
}

/// One derived join table with its two key columns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedJoin {
    pub(crate) relation: crate::scenario::id::SemanticId,
    pub(crate) name: StorageName,
    pub(crate) columns: Vec<DerivedColumn>,
    pub(crate) unique_pair: bool,
    pub(crate) on_owner_delete: OnDelete,
    pub(crate) on_target_delete: OnDelete,
}

impl DerivedJoin {
    /// The materialized relation.
    pub fn relation(&self) -> &crate::scenario::id::SemanticId {
        &self.relation
    }

    /// The join-table name.
    pub fn table(&self) -> &StorageName {
        &self.name
    }

    /// The two join-key columns, `[owner, target]`.
    pub fn columns(&self) -> &[DerivedColumn] {
        &self.columns
    }

    /// Whether the owner-target pair is unique.
    pub const fn unique_pair(&self) -> bool {
        self.unique_pair
    }

    /// The delete action on the owner key.
    pub const fn on_owner_delete(&self) -> OnDelete {
        self.on_owner_delete
    }

    /// The delete action on the target key.
    pub const fn on_target_delete(&self) -> OnDelete {
        self.on_target_delete
    }
}

/// One derived target-namespaced storage projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedProjection {
    pub(crate) namespace: Namespace,
    pub(crate) tables: Vec<DerivedTable>,
    pub(crate) joins: Vec<DerivedJoin>,
}

impl DerivedProjection {
    /// The target namespace.
    pub const fn namespace(&self) -> Namespace {
        self.namespace
    }

    /// The derived tables, byte-sorted by table name.
    pub fn tables(&self) -> &[DerivedTable] {
        &self.tables
    }

    /// The derived join tables, byte-sorted by table name.
    pub fn joins(&self) -> &[DerivedJoin] {
        &self.joins
    }

    /// Resolve one derived table by entity key.
    pub fn table(&self, entity: &EntityKey) -> Option<&DerivedTable> {
        self.tables.iter().find(|table| &table.entity == entity)
    }
}

/// Project the attachment onto one target namespace: the published
/// derivation from the same domain model. Pure and read-only; the
/// result is byte-identical for value-equal attachments.
pub fn project(
    attachment: &StorageProjectionAttachment,
    namespace: Namespace,
) -> Result<DerivedProjection, DiagnosticSet> {
    attachment.semantic_self_check()?;
    let projection = attachment.projection(namespace).ok_or_else(|| {
        diagnostic::rule_invalid(diagnostic::MAPPING_INVALID, "namespace-absent", None)
    })?;
    let mut tables: Vec<DerivedTable> = Vec::with_capacity(projection.tables().len());
    for table in projection.tables() {
        tables.push(derive_table(attachment, projection, table)?);
    }
    tables.sort_by(|left, right| left.name.cmp(&right.name));
    let mut joins: Vec<DerivedJoin> = Vec::with_capacity(projection.joins().len());
    for join in projection.joins() {
        joins.push(derive_join(attachment, projection, join)?);
    }
    joins.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(DerivedProjection {
        namespace,
        tables,
        joins,
    })
}

/// Derive the public DTO field list of one entity from the domain
/// declarations only. Returns `None` for an unknown entity key.
pub fn public_fields(
    attachment: &StorageProjectionAttachment,
    entity: &EntityKey,
) -> Option<Vec<FieldName>> {
    let entity = attachment.entity(entity)?;
    let mut fields: Vec<FieldName> = entity
        .fields()
        .iter()
        .filter(|field| field.visibility() == Visibility::Public)
        .map(|field| field.name().clone())
        .collect();
    fields.sort();
    Some(fields)
}

/// Derive one table.
fn derive_table(
    attachment: &StorageProjectionAttachment,
    projection: &Projection,
    table: &super::projection::Table,
) -> Result<DerivedTable, DiagnosticSet> {
    let entity = attachment.entity(&table.entity).ok_or_else(|| {
        diagnostic::rule_invalid(
            diagnostic::MAPPING_INVALID,
            "entity-absent",
            Some(table.entity.as_str()),
        )
    })?;
    let mut columns: Vec<DerivedColumn> = Vec::new();
    for field in entity.fields() {
        columns.push(DerivedColumn {
            name: column_name(field.name().as_str())?,
            storage_type: map_type(projection.namespace(), field.field_type()),
            nullable: !field.required(),
            origin: ColumnOrigin::Field,
            visibility: field.visibility(),
        });
    }
    for technical in table.technical_columns() {
        columns.push(DerivedColumn {
            name: technical.name().clone(),
            storage_type: technical.storage_type().as_str().to_owned(),
            nullable: technical.nullable(),
            origin: ColumnOrigin::Technical,
            visibility: Visibility::Private,
        });
    }
    for generated in table.generated_columns() {
        columns.push(DerivedColumn {
            name: generated.name().clone(),
            storage_type: generated
                .storage_type()
                .map(|storage_type| storage_type.as_str().to_owned())
                .unwrap_or_else(|| projection.namespace().big_integer().to_owned()),
            nullable: false,
            origin: ColumnOrigin::Generated,
            visibility: Visibility::Private,
        });
    }
    if let Some(column) = table.soft_delete() {
        columns.push(DerivedColumn {
            name: column.clone(),
            storage_type: projection.namespace().instant().to_owned(),
            nullable: true,
            origin: ColumnOrigin::SoftDelete,
            visibility: Visibility::Private,
        });
    }
    if let Some((column, storage_type)) = table.tenant_key() {
        columns.push(DerivedColumn {
            name: column.clone(),
            storage_type: storage_type.as_str().to_owned(),
            nullable: false,
            origin: ColumnOrigin::TenantKey,
            visibility: Visibility::Private,
        });
    }
    if let Some((created_at, updated_at)) = table.timestamps() {
        columns.push(DerivedColumn {
            name: created_at.clone(),
            storage_type: projection.namespace().instant().to_owned(),
            nullable: false,
            origin: ColumnOrigin::CreatedAt,
            visibility: Visibility::Private,
        });
        columns.push(DerivedColumn {
            name: updated_at.clone(),
            storage_type: projection.namespace().instant().to_owned(),
            nullable: false,
            origin: ColumnOrigin::UpdatedAt,
            visibility: Visibility::Private,
        });
    }
    let local_key = |table: &super::projection::Table, column: &StorageName| {
        local_column_type(attachment, projection, table, column)
    };
    // Foreign keys on this table: one per relation that places its key
    // here (target-side kinds) plus optional_reference owner keys.
    let mut foreign_keys: Vec<DerivedForeignKey> = Vec::new();
    let mut derived_key_columns: Vec<DerivedColumn> = Vec::new();
    let mut unique_foreign_keys: Vec<StorageName> = Vec::new();
    for relation in attachment.relations() {
        let placed_here = (relation.kind().target_foreign_key()
            && relation.target() == &table.entity)
            || (relation.kind().owner_foreign_key() && relation.owner() == &table.entity);
        if !placed_here {
            continue;
        }
        let column = relation.foreign_key().ok_or_else(|| {
            diagnostic::rule_invalid(
                diagnostic::MAPPING_INVALID,
                "foreign-key-absent",
                Some(relation.relation_id().as_str()),
            )
        })?;
        let referenced = if relation.kind().owner_foreign_key() {
            relation.target()
        } else {
            relation.owner()
        };
        let referenced_table = projection
            .tables()
            .iter()
            .find(|table| table.entity() == referenced)
            .ok_or_else(|| {
                diagnostic::rule_invalid(
                    diagnostic::MAPPING_INVALID,
                    "referenced-entity-unmapped",
                    Some(relation.relation_id().as_str()),
                )
            })?;
        let primary = single_primary_key(referenced_table)?;
        let storage_type = local_key(referenced_table, &primary)?;
        // A target-side foreign key is nullable only when the declared
        // behavior detaches (SET NULL); an owned child row always
        // carries its owner.
        let nullable = relation.delete_behavior() == DeleteBehavior::Detach;
        derived_key_columns.push(DerivedColumn {
            name: column.clone(),
            storage_type,
            nullable,
            origin: ColumnOrigin::ForeignKey,
            visibility: Visibility::Private,
        });
        if relation.kind() == RelationKind::OneToOne {
            unique_foreign_keys.push(column.clone());
        }
        foreign_keys.push(DerivedForeignKey {
            column: column.clone(),
            references_table: referenced_table.table().clone(),
            on_delete: OnDelete::of(relation.delete_behavior()),
        });
    }
    columns.append(&mut derived_key_columns);
    // Polymorphic materializations on this table.
    let mut polymorphics: Vec<DerivedPolymorphic> = Vec::new();
    for materialization in projection.polymorphics() {
        let relation = attachment
            .relations()
            .iter()
            .find(|relation| relation.relation_id() == materialization.relation())
            .ok_or_else(|| {
                diagnostic::rule_invalid(
                    diagnostic::MAPPING_INVALID,
                    "relation-absent",
                    Some(materialization.relation().as_str()),
                )
            })?;
        if relation.kind() != RelationKind::Polymorphic || relation.owner() != &table.entity {
            continue;
        }
        let target_table = projection
            .tables()
            .iter()
            .find(|table| table.entity() == relation.target())
            .ok_or_else(|| {
                diagnostic::rule_invalid(
                    diagnostic::MAPPING_INVALID,
                    "target-unmapped",
                    Some(relation.relation_id().as_str()),
                )
            })?;
        let primary = single_primary_key(target_table)?;
        let key_type = local_key(target_table, &primary)?;
        let nullable = relation.min() == 0;
        columns.push(DerivedColumn {
            name: materialization.key_column().clone(),
            storage_type: key_type,
            nullable,
            origin: ColumnOrigin::DiscriminatorKey,
            visibility: Visibility::Private,
        });
        columns.push(DerivedColumn {
            name: materialization.type_column().clone(),
            storage_type: format!("{}(64)", projection.namespace().discriminator()),
            nullable,
            origin: ColumnOrigin::DiscriminatorType,
            visibility: Visibility::Private,
        });
        polymorphics.push(DerivedPolymorphic {
            relation: materialization.relation().clone(),
            key_column: materialization.key_column().clone(),
            type_column: materialization.type_column().clone(),
        });
    }
    columns.sort_by(|left, right| left.name.cmp(&right.name));
    // A duplicate column name at this point means a declared storage
    // fact collides with a derived one; validation already refuses it,
    // so this is a defensive refusal, never a silent overwrite.
    if columns
        .windows(2)
        .any(|window| window[0].name == window[1].name)
    {
        return Err(diagnostic::rule_invalid(
            diagnostic::MAPPING_INVALID,
            "column-collision",
            Some(table.entity.as_str()),
        ));
    }
    foreign_keys.sort_by(|left, right| left.column.cmp(&right.column));
    let mut indexes: Vec<super::projection::Index> = table.indexes().to_vec();
    for column in unique_foreign_keys {
        indexes.push(super::projection::Index {
            name: None,
            columns: vec![column],
            unique: true,
        });
    }
    indexes.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.columns.cmp(&right.columns))
    });
    polymorphics.sort_by(|left, right| left.relation.cmp(&right.relation));
    Ok(DerivedTable {
        entity: table.entity().clone(),
        name: table.table().clone(),
        columns,
        primary_key: table.primary_key().to_vec(),
        foreign_keys,
        indexes,
        polymorphics,
    })
}

/// Derive one join table.
fn derive_join(
    attachment: &StorageProjectionAttachment,
    projection: &Projection,
    join: &super::projection::Join,
) -> Result<DerivedJoin, DiagnosticSet> {
    let relation = attachment
        .relations()
        .iter()
        .find(|relation| relation.relation_id() == join.relation())
        .ok_or_else(|| {
            diagnostic::rule_invalid(
                diagnostic::MAPPING_INVALID,
                "relation-absent",
                Some(join.relation().as_str()),
            )
        })?;
    let mut columns: Vec<DerivedColumn> = Vec::with_capacity(2);
    for (entity_key, column) in [
        (relation.owner(), &join.columns[0]),
        (relation.target(), &join.columns[1]),
    ]
    .into_iter()
    {
        let table = projection
            .tables()
            .iter()
            .find(|table| table.entity() == entity_key)
            .ok_or_else(|| {
                diagnostic::rule_invalid(
                    diagnostic::MAPPING_INVALID,
                    "join-entity-unmapped",
                    Some(entity_key.as_str()),
                )
            })?;
        let primary = single_primary_key(table)?;
        let storage_type = local_column_type(attachment, projection, table, &primary)?;
        columns.push(DerivedColumn {
            name: column.clone(),
            storage_type,
            nullable: false,
            origin: ColumnOrigin::JoinKey,
            visibility: Visibility::Private,
        });
    }
    Ok(DerivedJoin {
        relation: join.relation().clone(),
        name: join.table().clone(),
        columns,
        unique_pair: join.unique_pair(),
        on_owner_delete: join_delete_action(relation.delete_behavior()),
        on_target_delete: join_delete_action(relation.delete_behavior()),
    })
}

/// The join-table row action for one declared delete behavior: detach
/// severs the relation, which is the join row's deletion.
fn join_delete_action(behavior: DeleteBehavior) -> OnDelete {
    match behavior {
        DeleteBehavior::Cascade | DeleteBehavior::Detach => OnDelete::Cascade,
        DeleteBehavior::Restrict => OnDelete::Restrict,
    }
}

/// Resolve the single-column primary key of one declared table.
fn single_primary_key(table: &super::projection::Table) -> Result<StorageName, DiagnosticSet> {
    if table.primary_key().len() != 1 {
        return Err(diagnostic::rule_invalid(
            diagnostic::MAPPING_INVALID,
            "primary-key-not-single",
            Some(table.entity().as_str()),
        ));
    }
    Ok(table.primary_key()[0].clone())
}

/// Resolve one locally owned column's derived storage type. Foreign-key
/// columns never qualify: primary keys must be declared over locally
/// owned columns, which keeps resolution non-iterative.
fn local_column_type(
    attachment: &StorageProjectionAttachment,
    projection: &Projection,
    table: &super::projection::Table,
    column: &StorageName,
) -> Result<String, DiagnosticSet> {
    let entity = attachment.entity(&table.entity).ok_or_else(|| {
        diagnostic::rule_invalid(
            diagnostic::MAPPING_INVALID,
            "entity-absent",
            Some(table.entity.as_str()),
        )
    })?;
    for field in entity.fields() {
        if field.name().as_str() == column.as_str() {
            return Ok(map_type(projection.namespace(), field.field_type()));
        }
    }
    for technical in table.technical_columns() {
        if technical.name() == column {
            return Ok(technical.storage_type().as_str().to_owned());
        }
    }
    for generated in table.generated_columns() {
        if generated.name() == column {
            return Ok(generated
                .storage_type()
                .map(|storage_type| storage_type.as_str().to_owned())
                .unwrap_or_else(|| projection.namespace().big_integer().to_owned()));
        }
    }
    if table.soft_delete() == Some(column) {
        return Ok(projection.namespace().instant().to_owned());
    }
    if let Some((tenant, storage_type)) = table.tenant_key() {
        if tenant == column {
            return Ok(storage_type.as_str().to_owned());
        }
    }
    if let Some((created_at, updated_at)) = table.timestamps() {
        if created_at == column || updated_at == column {
            return Ok(projection.namespace().instant().to_owned());
        }
    }
    Err(diagnostic::rule_invalid(
        diagnostic::MAPPING_INVALID,
        "primary-key-unresolved",
        Some(table.entity().as_str()),
    ))
}

/// The published namespace type table: the exact rendering of one
/// domain value type in one namespace.
fn map_type(namespace: Namespace, field_type: &DomainType) -> String {
    match namespace {
        Namespace::Postgres => match field_type {
            DomainType::Boolean => "boolean".to_owned(),
            DomainType::Integer => "bigint".to_owned(),
            DomainType::Decimal { precision, scale } => {
                format!("numeric({precision},{scale})")
            }
            DomainType::String { length } => format!("varchar({length})"),
            DomainType::Text => "text".to_owned(),
            DomainType::Uuid => "uuid".to_owned(),
            DomainType::Date => "date".to_owned(),
            DomainType::Timestamp => "timestamptz".to_owned(),
            DomainType::Binary => "bytea".to_owned(),
            DomainType::Json => "jsonb".to_owned(),
        },
        Namespace::Laravel => match field_type {
            DomainType::Boolean => "boolean".to_owned(),
            DomainType::Integer => "biginteger".to_owned(),
            DomainType::Decimal { precision, scale } => {
                format!("decimal({precision},{scale})")
            }
            DomainType::String { length } => format!("string({length})"),
            DomainType::Text => "text".to_owned(),
            DomainType::Uuid => "uuid".to_owned(),
            DomainType::Date => "date".to_owned(),
            DomainType::Timestamp => "datetime".to_owned(),
            DomainType::Binary => "binary".to_owned(),
            DomainType::Json => "json".to_owned(),
        },
    }
}

/// Parse one field name as a column name; the grammars coincide.
fn column_name(field: &str) -> Result<StorageName, DiagnosticSet> {
    StorageName::parse(field).map_err(|_| {
        diagnostic::rule_invalid(diagnostic::DOMAIN_INVALID, "field-name", Some(field))
    })
}
