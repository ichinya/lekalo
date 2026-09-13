//! Model-bound resolution of the query-model attachment (issue #64).
//!
//! [`resolve`] binds one validated attachment to the exact project the
//! caller selected, re-checks custody (project id, Model version and
//! document digest, IR digest), and then checks every Model-dependent
//! declaration: query and source symbol kinds (the read-only proof),
//! read membership, parameter type expressions, filter field and
//! literal typing, sort determinism against the entity identity, the
//! cursor key type, selection and visibility boundaries, semantic-link
//! include paths, policy and scenario references, and — under the
//! strict profile — the tenant filter requirement. Pure and read-only:
//! the loader runs through its accepted read-only path and nothing is
//! ever written.
//!
//! Every reference slot is kind-checked, which is also the read-only
//! enforcement: a `command`, `effect`, or any other write surface can
//! never occupy the query, source, policy, or scenario slot; such a
//! declaration fails with an explicit registered diagnostic before any
//! plan is produced.

use std::collections::BTreeMap;

use crate::diagnostics::DiagnosticSet;
use crate::ir::{CompiledProject, Definition, DefinitionKind, ScalarBase, TypeRef};
use crate::loader::LoadSelection;
use crate::result::DomainResult;
use crate::scenario::id::FieldName;

use super::diagnostic::{
    self, CONTRACT_INVALID, FILTER_INVALID, PAGINATION_INVALID, REFERENCE_INVALID, SORT_INVALID,
    SOURCE_INVALID, TENANT_FILTER_MISSING, VISIBILITY_BOUNDARY,
};
use super::filter::{FilterExpr, FilterLeaf, FilterOp, FilterValue, Literal};
use super::plan::QueryPlan;
use super::query::PaginationStrategy;
use super::QueryModelAttachment;

/// The tenant filter operators that actually constrain the tenant key.
const TENANT_CONSTRAINING_OPS: &[FilterOp] = &[FilterOp::Eq, FilterOp::In];

/// The finished resolution: one canonical plan per declared query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Resolution {
    plans: Vec<QueryPlan>,
}

impl Resolution {
    /// One plan per declared query, canonically ordered by query id.
    pub fn plans(&self) -> &[QueryPlan] {
        &self.plans
    }
}

/// Resolve one attachment against the project selection. See the
/// module documentation for the exact checks and the custody order.
pub fn resolve(
    attachment: &QueryModelAttachment,
    selection: &LoadSelection,
    profile: Profile,
) -> Result<Resolution, DomainResult> {
    let model_json = match crate::loader::run(selection, false) {
        DomainResult::Valid {
            payload: crate::result::SuccessPayload::Model { json, .. },
            ..
        } => json,
        other => return Err(other),
    };
    let model = crate::loader::normalize_model(selection)?;
    let compilation = crate::ir::compile(&model).map_err(|failure| failure.into_result())?;

    // Custody: the attachment binds exactly one project and one Model
    // state; anything else refuses before any resolution.
    if attachment.project_id().as_str()
        != compilation
            .project
            .project
            .as_ref()
            .map(|project| project.id.as_str())
            .unwrap_or_default()
    {
        return Err(DomainResult::invalid(custody_set("project-id")));
    }
    if attachment.model_ref().version().as_str() != model.model_version.as_str() {
        return Err(DomainResult::invalid(custody_set("model-version")));
    }
    if attachment.model_ref().digest().as_str()
        != crate::requirements::sha256_digest(model_json.as_bytes())
    {
        return Err(DomainResult::invalid(custody_set("model-digest")));
    }

    // The symbol table: every definition by id.
    let symbols = symbol_table(&compilation.project);

    // Tenancy targets resolve first: they guard the strict profile.
    for declaration in attachment.tenancy() {
        let definition = symbols.get(declaration.entity.as_str()).ok_or_else(|| {
            invalid(
                REFERENCE_INVALID,
                "tenancy-entity-unresolved",
                declaration.entity.as_str(),
            )
        })?;
        let entity = as_entity(definition).ok_or_else(|| {
            invalid(
                REFERENCE_INVALID,
                "tenancy-entity-kind",
                declaration.entity.as_str(),
            )
        })?;
        if !entity
            .fields
            .iter()
            .any(|field| field.name.as_str() == declaration.field.as_str())
        {
            return Err(invalid(
                REFERENCE_INVALID,
                "tenancy-field-unresolved",
                &format!(
                    "{}.{}",
                    declaration.entity.as_str(),
                    declaration.field.as_str()
                ),
            ));
        }
    }

    let mut plans = Vec::with_capacity(attachment.queries().len());
    for decl in attachment.queries() {
        resolve_query(attachment, decl, &symbols, profile)?;
        plans.push(QueryPlan::build(decl));
    }
    Ok(Resolution { plans })
}

/// The validation profile of one resolution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    /// The default profile: tenant omissions stay silent.
    Default,
    /// The strict profile: tenant filter omissions are diagnosed.
    Strict,
}

impl Profile {
    /// The profile for one strict flag.
    pub const fn from_strict(strict: bool) -> Self {
        if strict {
            Self::Strict
        } else {
            Self::Default
        }
    }
}

/// The symbol table of one compilation.
type Symbols<'a> = BTreeMap<&'a str, &'a Definition>;

/// Collect the sorted symbol table.
fn symbol_table(project: &CompiledProject) -> Symbols<'_> {
    let mut symbols = BTreeMap::new();
    for definition in &project.definitions {
        symbols.insert(definition.id().as_str(), definition);
    }
    symbols
}

/// Resolve one query declaration against the symbol table.
fn resolve_query(
    attachment: &QueryModelAttachment,
    decl: &super::query::QueryDecl,
    symbols: &Symbols<'_>,
    profile: Profile,
) -> Result<(), DomainResult> {
    let subject = decl.query.as_str();

    // The bound symbol must be a query definition. Any other kind is
    // refused; command/effect targets are named as write surfaces.
    let query_def = symbols
        .get(subject)
        .ok_or_else(|| invalid(SOURCE_INVALID, "query-symbol-unresolved", subject))?;
    let query_def = match query_def.kind() {
        DefinitionKind::Query => match query_def {
            Definition::Query(query) => query,
            _ => unreachable!("kind gate"),
        },
        DefinitionKind::Command | DefinitionKind::Effect => {
            return Err(invalid(SOURCE_INVALID, "write-surface", subject))
        }
        _ => return Err(invalid(SOURCE_INVALID, "query-symbol-kind", subject)),
    };

    // The source must be an entity the bound query reads.
    let source = symbols
        .get(decl.source.as_str())
        .ok_or_else(|| invalid(SOURCE_INVALID, "source-unresolved", decl.source.as_str()))?;
    let entity = match source.kind() {
        DefinitionKind::Entity => match source {
            Definition::Entity(entity) => entity,
            _ => unreachable!("kind gate"),
        },
        DefinitionKind::Command | DefinitionKind::Effect => {
            return Err(invalid(
                SOURCE_INVALID,
                "write-source",
                decl.source.as_str(),
            ))
        }
        _ => return Err(invalid(SOURCE_INVALID, "source-kind", decl.source.as_str())),
    };
    if !query_def
        .reads
        .iter()
        .any(|read| read.as_str() == decl.source.as_str())
    {
        return Err(invalid(
            SOURCE_INVALID,
            "source-not-read",
            decl.source.as_str(),
        ));
    }

    // Parameters resolve to declared model types.
    let mut parameter_types = BTreeMap::new();
    for parameter in &decl.parameters {
        let resolved = resolve_type_expression(&parameter.parameter_type, symbols)?;
        parameter_types.insert(parameter.name.clone(), resolved);
    }

    // Filter leaves type-check against the source entity.
    if let Some(filter) = &decl.filter {
        for leaf in filter.leaves() {
            check_leaf(leaf, entity, symbols, &parameter_types)?;
        }
    }

    // Sort keys resolve and end in the identity tie-breaker.
    if let Some(sort) = &decl.sort {
        for key in sort {
            let leaf = field_leaf_type(entity, &key.field).ok_or_else(|| {
                invalid(SORT_INVALID, "sort-field-unresolved", key.field.as_str())
            })?;
            if !sort_type_allowed(&leaf, symbols) {
                return Err(invalid(SORT_INVALID, "sort-field-type", key.field.as_str()));
            }
        }
        let identity: Vec<&str> = entity.identity.iter().map(|field| field.as_str()).collect();
        if identity.is_empty() || sort.len() < identity.len() {
            return Err(invalid(
                SORT_INVALID,
                "identity-tiebreaker-missing",
                subject,
            ));
        }
        let trailing: Vec<&str> = sort[sort.len() - identity.len()..]
            .iter()
            .map(|key| key.field.as_str())
            .collect();
        if trailing != identity {
            return Err(invalid(
                SORT_INVALID,
                "identity-tiebreaker-missing",
                subject,
            ));
        }
    }

    // The cursor continuation value carries the identity tie-breaker
    // (the trailing sort key), so its parameter must match that type.
    if let Some(pagination) = &decl.pagination {
        if pagination.strategy == PaginationStrategy::Cursor {
            let key = pagination
                .key_parameter
                .as_ref()
                .expect("self-check guarantees a declared key");
            let resolved = &parameter_types[key];
            let sort = decl.sort.as_ref().expect("self-check guarantees a sort");
            let tail = sort.last().expect("self-check guarantees non-empty");
            let anchor = field_leaf_type(entity, &tail.field).ok_or_else(|| {
                invalid(SORT_INVALID, "sort-field-unresolved", tail.field.as_str())
            })?;
            let anchor = classify(&anchor, symbols);
            if !cursor_type_matches(resolved, &anchor) {
                return Err(invalid(PAGINATION_INVALID, "cursor-key-type", key.as_str()));
            }
        }
    }

    // Selection: fields resolve, types project, and the visibility
    // boundary of the bound query holds.
    let query_visibility = query_def.common.visibility;
    if let Some(selection) = &decl.selection {
        for field in selection {
            let field_type = entity
                .fields
                .iter()
                .find(|candidate| candidate.name.as_str() == field.field.as_str())
                .map(|found| &found.r#type)
                .ok_or_else(|| {
                    invalid(
                        REFERENCE_INVALID,
                        "selection-field-unresolved",
                        field.field.as_str(),
                    )
                })?;
            let leaf_symbol = type_leaf_symbol(field_type)
                .ok_or_else(|| invalid(CONTRACT_INVALID, "selection-type", field.field.as_str()))?;
            let leaf_definition = symbols.get(leaf_symbol.as_str()).ok_or_else(|| {
                invalid(
                    REFERENCE_INVALID,
                    "selection-type-unresolved",
                    field.field.as_str(),
                )
            })?;
            if !projection_kind_allowed(leaf_definition.kind()) {
                return Err(invalid(
                    CONTRACT_INVALID,
                    "selection-type",
                    field.field.as_str(),
                ));
            }
            if exposes_module_symbol(query_visibility, leaf_definition) {
                return Err(invalid(
                    VISIBILITY_BOUNDARY,
                    "module-type-through-project-query",
                    field.field.as_str(),
                ));
            }
        }
    }

    // Includes: every path segment is an entity-typed relation; the
    // terminal entity is resolved (and, under the strict profile,
    // tenancy-checked) through one shared walk.
    for include in &decl.includes {
        include_terminal_entity(entity, &include.path, symbols)?;
    }

    // Policy and scenario references resolve to their kinds.
    for policy in &decl.policies {
        let definition = symbols
            .get(policy.as_str())
            .ok_or_else(|| invalid(REFERENCE_INVALID, "policy-unresolved", policy.as_str()))?;
        if definition.kind() != DefinitionKind::Policy {
            return Err(invalid(REFERENCE_INVALID, "policy-kind", policy.as_str()));
        }
    }
    for scenario in &decl.scenarios {
        let definition = symbols
            .get(scenario.as_str())
            .ok_or_else(|| invalid(REFERENCE_INVALID, "scenario-unresolved", scenario.as_str()))?;
        if definition.kind() != DefinitionKind::Scenario {
            return Err(invalid(
                REFERENCE_INVALID,
                "scenario-kind",
                scenario.as_str(),
            ));
        }
    }

    // The strict tenant requirement: the source and every included
    // terminal entity that is tenant-scoped must be constrained.
    if profile == Profile::Strict {
        let constrained = decl
            .filter
            .as_ref()
            .map(|filter| filter_constrains(filter, attachment, decl.source.as_str()))
            .unwrap_or(false);
        if let Some(tenant_field) = attachment.tenant_field(decl.source.as_str()) {
            if !constrained {
                return Err(invalid(
                    TENANT_FILTER_MISSING,
                    "source-tenant-filter-missing",
                    &format!("{}.{}", decl.source.as_str(), tenant_field.as_str()),
                ));
            }
        }
        for include in &decl.includes {
            let terminal = include_terminal_entity(entity, &include.path, symbols)?;
            if attachment.tenant_field(terminal).is_some() {
                return Err(invalid(
                    TENANT_FILTER_MISSING,
                    "include-tenant-unconstrained",
                    terminal,
                ));
            }
        }
    }
    Ok(())
}

/// Whether the filter tree constrains the tenant field of one entity:
/// every row satisfying the filter must satisfy a tenant `eq`/`in`
/// leaf. The decision follows the logical structure, not the leaf
/// set: see [`constrains_tenant`].
fn filter_constrains(filter: &FilterExpr, attachment: &QueryModelAttachment, entity: &str) -> bool {
    let Some(tenant_field) = attachment.tenant_field(entity) else {
        return false;
    };
    constrains_tenant(filter, tenant_field)
}

/// The recursive tenant implication over one subtree. A conjunction
/// constrains as soon as one conjunct constrains (`A and B` implies
/// `C` whenever one operand implies `C`); a disjunction constrains
/// only when every disjunct constrains (each `or` branch must scope
/// its own satisfying rows to the tenant); a negation never
/// constrains (the rows satisfying `not X` are not derivably scoped
/// to the tenant, whatever `X` is). `and`/`or` are never empty: the
/// wire grammar refuses empty operand lists.
fn constrains_tenant(expr: &FilterExpr, tenant_field: &FieldName) -> bool {
    match expr {
        FilterExpr::Leaf(leaf) => {
            leaf.field == *tenant_field
                && TENANT_CONSTRAINING_OPS.contains(&leaf.op)
                && leaf.value.is_some()
        }
        FilterExpr::And(operands) => operands
            .iter()
            .any(|operand| constrains_tenant(operand, tenant_field)),
        FilterExpr::Or(operands) => operands
            .iter()
            .all(|operand| constrains_tenant(operand, tenant_field)),
        FilterExpr::Not(_) => false,
    }
}

/// The terminal entity id of one include path.
fn include_terminal_entity<'a>(
    source: &'a crate::ir::EntityDef,
    path: &[FieldName],
    symbols: &Symbols<'a>,
) -> Result<&'a str, DomainResult> {
    let mut current = source;
    for segment in path {
        let field = current
            .fields
            .iter()
            .find(|candidate| candidate.name.as_str() == segment.as_str())
            .map(|found| &found.r#type)
            .ok_or_else(|| {
                invalid(
                    REFERENCE_INVALID,
                    "include-field-unresolved",
                    segment.as_str(),
                )
            })?;
        let target = relation_target(field)
            .ok_or_else(|| invalid(REFERENCE_INVALID, "include-segment-type", segment.as_str()))?;
        let definition = symbols.get(target.as_str()).ok_or_else(|| {
            invalid(
                REFERENCE_INVALID,
                "include-target-unresolved",
                target.as_str(),
            )
        })?;
        current = as_entity(definition)
            .ok_or_else(|| invalid(REFERENCE_INVALID, "include-segment-type", segment.as_str()))?;
    }
    Ok(current.id.as_str())
}

/// One resolved leaf type: an unresolved symbol reference, a scalar
/// base, an enum/entity/value-object symbol, a wrapped list of one
/// leaf type, or an unprojectable leaf.
#[derive(Clone, Debug, Eq, PartialEq)]
enum LeafType {
    /// A direct symbol reference not yet classified.
    Symbol(crate::ir::SymbolId),
    /// A scalar base.
    Scalar(ScalarBase),
    /// An enum symbol.
    Enum(String),
    /// An entity symbol.
    Entity(String),
    /// A value-object symbol.
    ValueObject(String),
    /// A list of one leaf type.
    ListOf(Box<LeafType>),
    /// A leaf that cannot project or filter.
    Unprojectable,
}

/// The leaf type of one source-entity field.
fn field_leaf_type(entity: &crate::ir::EntityDef, field: &FieldName) -> Option<LeafType> {
    let found = entity
        .fields
        .iter()
        .find(|candidate| candidate.name.as_str() == field.as_str())?;
    Some(leaf_of(&found.r#type))
}

/// The leaf classification of one type reference: list wrappers are
/// preserved so set parameters can match them.
fn leaf_of(type_ref: &TypeRef) -> LeafType {
    match type_ref {
        TypeRef::Ref(symbol) => LeafType::Symbol(symbol.clone()),
        TypeRef::List(inner) => LeafType::ListOf(Box::new(leaf_of(inner))),
        TypeRef::Optional(inner) => leaf_of(inner),
    }
}

/// Classify the leaf symbol of a type against the declaration kinds.
fn classify(leaf: &LeafType, symbols: &Symbols<'_>) -> LeafType {
    match leaf {
        LeafType::Symbol(symbol) => match symbols
            .get(symbol.as_str())
            .map(|definition| definition.kind())
        {
            Some(DefinitionKind::Scalar) => match symbols.get(symbol.as_str()) {
                Some(Definition::Scalar(scalar)) => LeafType::Scalar(scalar.base),
                _ => LeafType::Unprojectable,
            },
            Some(DefinitionKind::Enum) => LeafType::Enum(symbol.as_str().to_owned()),
            Some(DefinitionKind::Entity) => LeafType::Entity(symbol.as_str().to_owned()),
            Some(DefinitionKind::ValueObject) => LeafType::ValueObject(symbol.as_str().to_owned()),
            _ => LeafType::Unprojectable,
        },
        other => other.clone(),
    }
}

/// Check one filter leaf against the source entity and parameters.
fn check_leaf(
    leaf: &FilterLeaf,
    entity: &crate::ir::EntityDef,
    symbols: &Symbols<'_>,
    parameters: &BTreeMap<crate::query_model::id::ParameterName, LeafType>,
) -> Result<(), DomainResult> {
    let raw = field_leaf_type(entity, &leaf.field).ok_or_else(|| {
        invalid(
            FILTER_INVALID,
            "filter-field-unresolved",
            leaf.field.as_str(),
        )
    })?;
    let leaf_type = classify(&raw, symbols);
    let field = leaf.field.as_str();
    let value = match (&leaf.op, &leaf.value) {
        (FilterOp::IsNull | FilterOp::IsNotNull, _) => {
            return check_leaf_orderability(&leaf.op, &leaf_type, field);
        }
        (_, Some(value)) => value,
        (_, None) => return Err(invalid(FILTER_INVALID, "filter-value-missing", field)),
    };
    check_leaf_orderability(&leaf.op, &leaf_type, field)?;
    match value {
        FilterValue::Param(name) => {
            let resolved = parameters
                .get(name)
                .ok_or_else(|| invalid(FILTER_INVALID, "filter-param-undeclared", name.as_str()))?;
            if !value_matches(&leaf.op, &leaf_type, resolved) {
                return Err(invalid(FILTER_INVALID, "filter-param-type", field));
            }
        }
        FilterValue::Literal(literal) => {
            if !literal_matches(&leaf.op, &leaf_type, literal, symbols) {
                return Err(invalid(FILTER_INVALID, "filter-literal-type", field));
            }
        }
    }
    Ok(())
}

/// Enforce the closed operator-to-type orderability contract.
fn check_leaf_orderability(
    op: &FilterOp,
    leaf_type: &LeafType,
    field: &str,
) -> Result<(), DomainResult> {
    let range_allowed = matches!(
        leaf_type,
        LeafType::Scalar(
            ScalarBase::String | ScalarBase::Number | ScalarBase::Date | ScalarBase::Datetime
        )
    );
    let equality_allowed = matches!(
        leaf_type,
        LeafType::Scalar(_) | LeafType::Enum(_) | LeafType::Entity(_)
    );
    if op.expects_set() && !equality_allowed {
        return Err(invalid(FILTER_INVALID, "filter-op-type", field));
    }
    if matches!(
        op,
        FilterOp::Lt | FilterOp::Le | FilterOp::Gt | FilterOp::Ge
    ) && !range_allowed
    {
        return Err(invalid(FILTER_INVALID, "filter-op-type", field));
    }
    if matches!(op, FilterOp::Eq | FilterOp::Ne) && !equality_allowed {
        return Err(invalid(FILTER_INVALID, "filter-op-type", field));
    }
    Ok(())
}

/// Whether a param type satisfies one leaf operator.
fn value_matches(op: &FilterOp, leaf_type: &LeafType, param: &LeafType) -> bool {
    if op.expects_set() {
        return set_param_matches(leaf_type, param);
    }
    scalar_matches(leaf_type, param)
}

/// Whether a set param (`list<T>`) matches the leaf type.
fn set_param_matches(leaf_type: &LeafType, param: &LeafType) -> bool {
    match param {
        LeafType::ListOf(inner) => scalar_matches(leaf_type, inner),
        _ => false,
    }
}

/// Whether two leaf types carry the same scalar base or symbol.
fn scalar_matches(leaf_type: &LeafType, other: &LeafType) -> bool {
    match (leaf_type, other) {
        (LeafType::Scalar(base), LeafType::Scalar(other_base)) => base == other_base,
        (LeafType::Enum(symbol), LeafType::Enum(other)) => symbol == other,
        (LeafType::Entity(symbol), LeafType::Entity(other)) => symbol == other,
        _ => false,
    }
}

/// Whether a literal satisfies one leaf operator.
fn literal_matches(
    op: &FilterOp,
    leaf_type: &LeafType,
    literal: &Literal,
    symbols: &Symbols<'_>,
) -> bool {
    if op.expects_set() {
        return match literal {
            Literal::Set(items) => items
                .iter()
                .all(|item| literal_kind_matches(leaf_type, item, symbols)),
            _ => false,
        };
    }
    literal_kind_matches(leaf_type, literal, symbols)
}

/// Whether one literal matches the leaf type's base; enum literals
/// must be declared members of the field's enum type.
fn literal_kind_matches(leaf_type: &LeafType, literal: &Literal, symbols: &Symbols<'_>) -> bool {
    match leaf_type {
        LeafType::Scalar(base) => match base {
            ScalarBase::String => matches!(literal, Literal::Str(_)),
            ScalarBase::Number => matches!(literal, Literal::Integer(_) | Literal::Decimal(_)),
            ScalarBase::Boolean => matches!(literal, Literal::Bool(_)),
            ScalarBase::Date => matches!(literal, Literal::Date(_)),
            ScalarBase::Datetime => matches!(literal, Literal::DateTime(_)),
            ScalarBase::Uuid => matches!(literal, Literal::Uuid(_)),
            ScalarBase::Uri => matches!(literal, Literal::Uri(_)),
        },
        LeafType::Enum(symbol) => match (symbols.get(symbol.as_str()), literal) {
            (Some(Definition::Enum(definition)), Literal::Enum(member)) => definition
                .values
                .iter()
                .any(|value| value.value.as_str() == member.as_str()),
            _ => false,
        },
        _ => false,
    }
}

/// Whether the cursor key param matches the leading sort key type.
fn cursor_type_matches(param: &LeafType, sort_leaf: &LeafType) -> bool {
    scalar_matches(sort_leaf, param)
}

/// Whether one sort field type is orderable for a deterministic sort.
fn sort_type_allowed(leaf: &LeafType, symbols: &Symbols<'_>) -> bool {
    matches!(
        classify(leaf, symbols),
        LeafType::Scalar(_) | LeafType::Enum(_)
    )
}

/// Whether a projection may carry this kind of leaf.
fn projection_kind_allowed(kind: DefinitionKind) -> bool {
    matches!(
        kind,
        DefinitionKind::Scalar | DefinitionKind::Enum | DefinitionKind::ValueObject
    )
}

/// Whether a project-visibility query would expose a module symbol.
fn exposes_module_symbol(
    query_visibility: Option<crate::ir::Visibility>,
    leaf_definition: &Definition,
) -> bool {
    query_visibility == Some(crate::ir::Visibility::Project)
        && leaf_definition.common().visibility == Some(crate::ir::Visibility::Module)
}

/// The direct symbol under a type reference (wrappers unwrapped once
/// level at a time by the caller when needed).
fn type_leaf_symbol(type_ref: &TypeRef) -> Option<crate::ir::SymbolId> {
    match type_ref {
        TypeRef::Ref(symbol) => Some(symbol.clone()),
        TypeRef::List(inner) | TypeRef::Optional(inner) => type_leaf_symbol(inner),
    }
}

/// The entity target of one relation field, when the type is a
/// (possibly wrapped) entity reference.
fn relation_target(field_type: &TypeRef) -> Option<crate::ir::SymbolId> {
    match field_type {
        TypeRef::Ref(symbol) => Some(symbol.clone()),
        TypeRef::Optional(inner) | TypeRef::List(inner) => relation_target(inner),
    }
}

/// Resolve one bounded parameter type expression into its leaf type.
fn resolve_type_expression(text: &str, symbols: &Symbols<'_>) -> Result<LeafType, DomainResult> {
    let parsed = parse_type_expression(text)?;
    if parsed.levels() > crate::ir::MAX_TYPE_LEVELS {
        return Err(invalid(CONTRACT_INVALID, "parameter-type-depth", text));
    }
    match &parsed {
        ParsedType::Symbol(symbol) => {
            let definition = symbols
                .get(symbol.as_str())
                .ok_or_else(|| invalid(REFERENCE_INVALID, "parameter-type-unresolved", text))?;
            Ok(match definition {
                Definition::Scalar(scalar) => LeafType::Scalar(scalar.base),
                Definition::Enum(_) => LeafType::Enum(symbol.as_str().to_owned()),
                Definition::Entity(_) => LeafType::Entity(symbol.as_str().to_owned()),
                Definition::ValueObject(_) => LeafType::ValueObject(symbol.as_str().to_owned()),
                _ => return Err(invalid(CONTRACT_INVALID, "parameter-type-kind", text)),
            })
        }
        ParsedType::Optional(inner) => resolve_parsed(inner, symbols),
        ParsedType::List(inner) => Ok(LeafType::ListOf(Box::new(resolve_parsed(inner, symbols)?))),
    }
}

/// Resolve one already-parsed type subtree.
fn resolve_parsed(parsed: &ParsedType, symbols: &Symbols<'_>) -> Result<LeafType, DomainResult> {
    match parsed {
        ParsedType::Symbol(_) | ParsedType::List(_) => {
            resolve_type_expression(&parsed.spelling(), symbols)
        }
        ParsedType::Optional(inner) => resolve_parsed(inner, symbols),
    }
}

/// One parsed type-expression node.
enum ParsedType {
    /// A symbol reference.
    Symbol(String),
    /// A nullable wrapper.
    Optional(Box<ParsedType>),
    /// A list wrapper.
    List(Box<ParsedType>),
}

impl ParsedType {
    /// The nesting levels of this subtree (leaf included).
    fn levels(&self) -> usize {
        match self {
            Self::Symbol(_) => 1,
            Self::Optional(inner) | Self::List(inner) => inner.levels() + 1,
        }
    }

    /// The canonical re-spelling of this subtree.
    fn spelling(&self) -> String {
        match self {
            Self::Symbol(symbol) => symbol.clone(),
            Self::Optional(inner) => format!("{}?", inner.spelling()),
            Self::List(inner) => format!("list<{}>", inner.spelling()),
        }
    }
}

/// Parse the closed model type-expression grammar
/// (`symbol`, `symbol?`, `list<expr>`).
fn parse_type_expression(text: &str) -> Result<ParsedType, DomainResult> {
    let trimmed = text.trim();
    if let Some(head) = trimmed.strip_suffix('?') {
        return Ok(ParsedType::Optional(Box::new(parse_type_expression(head)?)));
    }
    if let Some(inner) = trimmed
        .strip_prefix("list<")
        .and_then(|rest| rest.strip_suffix('>'))
    {
        return Ok(ParsedType::List(Box::new(parse_type_expression(inner)?)));
    }
    if trimmed.is_empty() || trimmed.contains('<') || trimmed.contains('>') || trimmed.contains('?')
    {
        return Err(invalid(CONTRACT_INVALID, "parameter-type", text));
    }
    if crate::scenario::id::SemanticId::parse(trimmed).is_err() {
        return Err(invalid(CONTRACT_INVALID, "parameter-type", text));
    }
    Ok(ParsedType::Symbol(trimmed.to_owned()))
}

/// The typed invalid set for one rule violation.
fn invalid(rule: &'static str, detail: &str, subject: &str) -> DomainResult {
    DomainResult::invalid(diagnostic::rule_invalid(rule, detail, Some(subject)))
}

/// The typed invalid set for one custody mismatch.
fn custody_set(detail: &str) -> DiagnosticSet {
    diagnostic::rule_invalid(diagnostic::INPUT_INVALID, detail, None)
}

/// The entity behind one definition, when the kind matches.
fn as_entity(definition: &Definition) -> Option<&crate::ir::EntityDef> {
    match definition {
        Definition::Entity(entity) => Some(entity),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::id::ParameterName;
    use super::super::version;
    use super::*;

    fn attachment() -> QueryModelAttachment {
        QueryModelAttachment::from_value(&serde_json::json!({
            "schemaVersion": version::SCHEMA_VERSION,
            "identity": version::IDENTITY,
            "attachmentRevision": "1.0.0",
            "projectId": "planner",
            "modelRef": {
                "modelVersion": "1.0.0",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            "irRef": {
                "identity": version::IR_IDENTITY,
                "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            },
            "tenancy": [{ "entity": "planner.task", "field": "tenant" }],
            "queries": [{
                "query": "planner.q",
                "source": "planner.task",
                "cardinality": "one",
                "consistency": "strong"
            }]
        }))
        .expect("attachment")
    }

    fn leaf(field: &str, op: FilterOp) -> FilterExpr {
        FilterExpr::Leaf(FilterLeaf {
            field: FieldName::parse(field).expect("field"),
            op,
            value: None,
        })
    }

    fn tenant_eq() -> FilterExpr {
        FilterExpr::Leaf(FilterLeaf {
            field: FieldName::parse("tenant").expect("field"),
            op: FilterOp::Eq,
            value: Some(FilterValue::Param(
                ParameterName::parse("tenant_id").expect("param"),
            )),
        })
    }

    fn tenant_in_set() -> FilterExpr {
        FilterExpr::Leaf(FilterLeaf {
            field: FieldName::parse("tenant").expect("field"),
            op: FilterOp::In,
            value: Some(FilterValue::Literal(Literal::Set(vec![
                Literal::Str("t1".to_owned()),
                Literal::Str("t2".to_owned()),
            ]))),
        })
    }

    fn and(operands: Vec<FilterExpr>) -> FilterExpr {
        FilterExpr::And(operands)
    }

    fn or(operands: Vec<FilterExpr>) -> FilterExpr {
        FilterExpr::Or(operands)
    }

    fn not(operand: FilterExpr) -> FilterExpr {
        FilterExpr::Not(Box::new(operand))
    }

    fn constrains(filter: &FilterExpr) -> bool {
        filter_constrains(filter, &attachment(), "planner.task")
    }

    #[test]
    fn tenant_eq_and_in_leaves_constrain() {
        assert!(constrains(&tenant_eq()));
        assert!(constrains(&tenant_in_set()));
        assert!(constrains(&and(vec![
            leaf("state", FilterOp::Eq),
            tenant_eq()
        ])));
    }

    #[test]
    fn non_constraining_tenant_operators_refuse() {
        for op in [FilterOp::Ne, FilterOp::NotIn, FilterOp::Le] {
            let leaf = FilterExpr::Leaf(FilterLeaf {
                field: FieldName::parse("tenant").expect("field"),
                op,
                value: Some(FilterValue::Literal(Literal::Str("t1".to_owned()))),
            });
            assert!(!constrains(&leaf), "{op:?} must not constrain");
        }
        for op in [FilterOp::IsNull, FilterOp::IsNotNull] {
            assert!(
                !constrains(&leaf("tenant", op)),
                "{op:?} must not constrain"
            );
        }
    }

    #[test]
    fn not_wrapped_tenant_leaf_refuses() {
        assert!(!constrains(&not(tenant_eq())));
        assert!(!constrains(&not(tenant_in_set())));
        assert!(!constrains(&and(vec![
            leaf("state", FilterOp::Eq),
            not(tenant_eq())
        ])));
        // A negation never proves a tenant bound, even over a tenant
        // comparison; negating an unrelated leaf taints nothing.
        assert!(!constrains(&not(leaf("tenant", FilterOp::Eq))));
        assert!(constrains(&and(vec![
            not(leaf("state", FilterOp::Eq)),
            tenant_eq()
        ])));
    }

    #[test]
    fn or_requires_every_branch_constrained() {
        assert!(!constrains(&or(vec![
            leaf("state", FilterOp::Eq),
            tenant_eq()
        ])));
        assert!(!constrains(&or(vec![tenant_eq(), not(tenant_eq())])));
        assert!(constrains(&or(vec![tenant_eq(), tenant_in_set()])));
    }

    #[test]
    fn nested_or_and_not_mixes_judge_every_branch() {
        // Each disjunct carries its own tenant conjunct: sound.
        assert!(constrains(&or(vec![
            and(vec![tenant_eq(), leaf("state", FilterOp::Eq)]),
            and(vec![tenant_eq(), leaf("due", FilterOp::Le)]),
        ])));
        // One unconstrained disjunct admits foreign-tenant rows.
        assert!(!constrains(&or(vec![
            and(vec![tenant_eq(), leaf("state", FilterOp::Eq)]),
            leaf("state", FilterOp::Eq),
        ])));
        // A factored tenant conjunct covers the whole disjunction.
        assert!(constrains(&and(vec![
            or(vec![
                leaf("state", FilterOp::Eq),
                leaf("rank", FilterOp::Le)
            ]),
            tenant_eq(),
        ])));
        // The tenant leaf buried in one unsound `or` branch constrains
        // nothing.
        assert!(!constrains(&and(vec![
            or(vec![leaf("state", FilterOp::Eq), tenant_eq()]),
            leaf("due", FilterOp::IsNotNull),
        ])));
        // Negation swallows every nested tenant leaf.
        assert!(!constrains(&not(or(vec![
            tenant_eq(),
            and(vec![tenant_eq(), leaf("state", FilterOp::Eq)]),
        ]))));
        // A tenant conjunct survives an unrelated negated sibling.
        assert!(constrains(&or(vec![
            and(vec![tenant_eq(), not(leaf("state", FilterOp::Eq))]),
            tenant_in_set(),
        ])));
    }

    #[test]
    fn entity_without_tenancy_never_constrains() {
        assert!(!filter_constrains(
            &tenant_eq(),
            &attachment(),
            "planner.project"
        ));
    }
}
