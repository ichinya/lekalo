//! Typed wire of the task-links integration attachment (issue #52).
//!
//! Parsing is fail-closed: canonical JSON bytes only, unknown members
//! rejected, duplicate JSON keys rejected before serde can erase them,
//! every collection sorted into semantic byte order, and every
//! digest-shaped pin validated exactly. The parsed value is the only
//! surface validation and the CLI see; raw input bytes never leak past
//! this module.

use serde_json::Value as Json;

use super::{diagnostic, version};

/// One accepted `sha256:` digest pin.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Digest(pub(crate) String);

impl Digest {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 71 || !text.starts_with("sha256:") {
            return None;
        }
        if !bytes[7..].iter().all(u8::is_ascii_hexdigit) {
            return None;
        }
        if text[7..].bytes().any(|b| b.is_ascii_uppercase()) {
            return None;
        }
        Some(Self(text.to_owned()))
    }

    /// The exact `sha256:` spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One exact Model/IR pin pair (the attachment binds one compilation).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ModelRef {
    /// The source Model version literal.
    pub model_version: String,
    /// The canonical IR digest of the pinned compilation.
    pub ir_digest: Digest,
}

/// One owner-contract pin: accepted family identity plus exact digest.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct OwnerPin {
    /// The pinned owner family (closed set of five).
    pub family: String,
    /// The exact accepted contract identity.
    pub identity: String,
    /// The exact accepted digest.
    pub digest: Digest,
}

/// One typed relation declaration: cardinality, ownership, and detach
/// semantics between the bound task aggregate and its external links.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Relation {
    /// Stable relation id (`tasklinks.rel.task_links`).
    pub id: String,
    /// Referenced entity (one local Task).
    pub target: String,
    /// Ordered local-side fields (workspace/task refs).
    pub local_fields: Vec<String>,
    /// Ordered target-side identity fields (same arity as local).
    pub target_fields: Vec<String>,
    /// Minimum cardinality (0 for optional links).
    pub min: u64,
    /// Maximum cardinality; `None` is the explicit `null` unbounded
    /// spelling (`0..N`). The member itself is required at the wire.
    pub max: Option<u64>,
    /// Which side owns the association.
    pub ownership: String,
    /// Detach behavior (history-preserving only).
    pub on_detach: String,
    /// Delete behavior (restrict or detach; never an external delete).
    pub on_delete: String,
}

/// One unique-constraint declaration: the exact external identity tuple.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct UniqueConstraint {
    /// Stable constraint id (`tasklinks.uq_external_identity`).
    pub id: String,
    /// Constrained entity.
    pub entity: String,
    /// Ordered constraint fields (the identity tuple, in order).
    pub fields: Vec<String>,
    /// Comparison rule (`exact-text` or a declared, pinned
    /// normalization).
    pub comparison: String,
    /// Normalization reference when `comparison` is declared.
    pub normalization_ref: Option<String>,
    /// Declared scope of the constraint.
    pub scope: String,
}

/// One provider contract snapshot declaration.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ProviderContract {
    /// Stable contract id (`tasklinks.provider_contract.github_issues`).
    pub contract_id: String,
    /// Provider identifier (public, synthetic-safe).
    pub provider_id: String,
    /// Mapped object type (`issue`).
    pub object_type: String,
    /// Reviewed upstream API/version pin.
    pub upstream_pin: String,
    /// Exact snapshot revision digest.
    pub revision: Digest,
    /// Request schema reference.
    pub request_schema_ref: String,
    /// Response schema reference.
    pub response_schema_ref: String,
    /// Error schema reference.
    pub error_schema_ref: String,
    /// Revision ordering guarantee.
    pub revision_ordering: String,
    /// Remote deduplication guarantee.
    pub remote_dedup: String,
}

/// One capability requirement bound to a provider contract.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CapabilityRequirement {
    /// Capability id (`tasklinks.cap.issue_update`).
    pub capability_id: String,
    /// The declaring provider contract.
    pub contract_id: String,
    /// `required` or `optional`.
    pub requirement: String,
    /// Capability revision digest.
    pub revision: Digest,
}

/// One field-level policy: inbound/outbound allow or deny per field.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct FieldPolicy {
    /// Stable policy id.
    pub policy_id: String,
    /// Bound entity.
    pub entity: String,
    /// Direction (`inbound`/`outbound`).
    pub direction: String,
    /// Decision (`allow`/`deny`).
    pub decision: String,
    /// Sorted field names.
    pub fields: Vec<String>,
    /// Actor class.
    pub actor: String,
    /// Referenced Model policy.
    pub policy_ref: String,
    /// Policy revision digest.
    pub revision: Digest,
}

/// One action-level policy: allow or deny per command action.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ActionPolicy {
    /// Stable policy id.
    pub policy_id: String,
    /// Bound action (a tasklinks command symbol).
    pub action: String,
    /// Decision (`allow`/`deny`).
    pub decision: String,
    /// Actor class.
    pub actor: String,
    /// Optional capability reference.
    pub capability_ref: Option<String>,
    /// Referenced Model policy.
    pub policy_ref: String,
    /// Policy revision digest.
    pub revision: Digest,
}

/// One operation binding: action to capability, policy, effect, and
/// transport guarantees.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct OperationBinding {
    /// Operation symbol (`tasklinks.request_action`).
    pub operation: String,
    /// The bound action (same symbol in v1).
    pub action: String,
    /// Capability reference.
    pub capability_ref: String,
    /// Action policy reference.
    pub action_policy_ref: String,
    /// Model effect reference.
    pub effect_ref: String,
    /// Classification (`public`/`internal`).
    pub classification: String,
    /// Bounded timeout in milliseconds.
    pub timeout_ms: u64,
    /// Retry guarantee.
    pub retry_guarantee: String,
}

/// The delivery rule block: durable outbox states and hard boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryRules {
    /// The exact nine-state outbox machine, in canonical order: the
    /// seven durable delivery states plus the two uncertainty states
    /// (`accepted_pending_ack`, `unknown_delivery`) the engine
    /// actually implements.
    pub outbox_states: Vec<String>,
    /// Bounded attempts before dead-letter.
    pub max_attempts: u64,
    /// Deterministic backoff in milliseconds.
    pub backoff_ms: u64,
    /// Lease duration in milliseconds.
    pub lease_ms: u64,
    /// Always false: the network call never joins a local transaction.
    pub network_inside_transaction: bool,
    /// Always false: no implicit external delete exists.
    pub external_delete_implicit: bool,
    /// Acceptance-before-ack policy.
    pub uncertainty_policy: String,
}

/// Explicit contribution switches for downstream consumers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Contributions {
    /// Graph export contribution.
    pub graph: bool,
    /// Impact contribution.
    pub impact: bool,
    /// Inspect contribution.
    pub inspect: bool,
    /// Scenario contribution.
    pub scenarios: bool,
    /// Effect-trace contribution.
    pub effect_trace: bool,
}

/// One bound entity with its identity fields.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EntityBinding {
    /// Bound entity symbol.
    pub entity: String,
    /// Identity field names (ordered).
    pub identity: Vec<String>,
    /// Whether the entity carries the external identity tuple.
    pub external_identity: bool,
}

/// The fully parsed, closed attachment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attachment {
    attachment_revision: Digest,
    project_id: String,
    model_ref: ModelRef,
    ir_digest: Digest,
    owner_pins: Vec<OwnerPin>,
    entities: Vec<EntityBinding>,
    relations: Vec<Relation>,
    unique_constraints: Vec<UniqueConstraint>,
    provider_contracts: Vec<ProviderContract>,
    capability_requirements: Vec<CapabilityRequirement>,
    field_policies: Vec<FieldPolicy>,
    action_policies: Vec<ActionPolicy>,
    operation_bindings: Vec<OperationBinding>,
    delivery_rules: DeliveryRules,
    contributions: Contributions,
}

impl Attachment {
    /// The declared attachment revision digest.
    pub fn attachment_revision(&self) -> &Digest {
        &self.attachment_revision
    }

    /// The bound project id.
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// The exact Model/IR pin.
    pub fn model_ref(&self) -> &ModelRef {
        &self.model_ref
    }

    /// The pinned IR digest.
    pub fn ir_digest(&self) -> &Digest {
        &self.ir_digest
    }

    /// The sorted owner-contract pins.
    pub fn owner_pins(&self) -> &[OwnerPin] {
        &self.owner_pins
    }

    /// The sorted bound entities.
    pub fn entities(&self) -> &[EntityBinding] {
        &self.entities
    }

    /// The sorted relations.
    pub fn relations(&self) -> &[Relation] {
        &self.relations
    }

    /// The sorted unique constraints.
    pub fn unique_constraints(&self) -> &[UniqueConstraint] {
        &self.unique_constraints
    }

    /// The sorted provider contracts.
    pub fn provider_contracts(&self) -> &[ProviderContract] {
        &self.provider_contracts
    }

    /// The sorted capability requirements.
    pub fn capability_requirements(&self) -> &[CapabilityRequirement] {
        &self.capability_requirements
    }

    /// The sorted field policies.
    pub fn field_policies(&self) -> &[FieldPolicy] {
        &self.field_policies
    }

    /// The sorted action policies.
    pub fn action_policies(&self) -> &[ActionPolicy] {
        &self.action_policies
    }

    /// The sorted operation bindings.
    pub fn operation_bindings(&self) -> &[OperationBinding] {
        &self.operation_bindings
    }

    /// The delivery rule block.
    pub fn delivery_rules(&self) -> &DeliveryRules {
        &self.delivery_rules
    }

    /// The contribution switches.
    pub fn contributions(&self) -> &Contributions {
        &self.contributions
    }

    /// Look up one relation by id.
    pub fn relation(&self, id: &str) -> Option<&Relation> {
        self.relations.iter().find(|r| r.id == id)
    }

    /// Look up one unique constraint by id.
    pub fn unique_constraint(&self, id: &str) -> Option<&UniqueConstraint> {
        self.unique_constraints.iter().find(|c| c.id == id)
    }

    /// Look up one provider contract by id.
    pub fn provider_contract(&self, id: &str) -> Option<&ProviderContract> {
        self.provider_contracts.iter().find(|c| c.contract_id == id)
    }
}

/// The closed top-level attachment member set, in canonical document
/// order. The revision digest covers every member except
/// `attachmentRevision` itself.
pub const ATTACHMENT_MEMBERS: [&str; 17] = [
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "ownerContractPins",
    "entities",
    "relations",
    "uniqueConstraints",
    "providerContracts",
    "linkCapabilityRequirements",
    "fieldPolicies",
    "actionPolicies",
    "operationBindings",
    "deliveryRules",
    "contributions",
];

/// Parse one attachment from exact JSON bytes.
pub fn parse(bytes: &[u8]) -> Result<Attachment, diagnostic::Failure> {
    if bytes.len() > version::MAX_BYTES {
        return Err(diagnostic::input_invalid("attachment-bound"));
    }
    let value: Json =
        super::json::parse(bytes).map_err(|_| diagnostic::input_invalid("attachment-json"))?;
    if !matches!(value, Json::Object(_)) {
        return Err(diagnostic::input_invalid("attachment-shape"));
    }
    let object = value.as_object().expect("checked above");
    for key in object.keys() {
        if !ATTACHMENT_MEMBERS.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let declared_version = object.get("schemaVersion").and_then(Json::as_str);
    let declared_identity = object.get("identity").and_then(Json::as_str);
    if declared_version != Some(version::SCHEMA_VERSION)
        || declared_identity != Some(version::IDENTITY)
    {
        // A well-formed discriminator of this same family at a
        // DIFFERENT version than the accepted one is the exit-5 class:
        // the document is shaped but its contract version is outside
        // the accepted registry. A corrupted identity under the
        // accepted discriminator is a plain invalid (exit 1), never a
        // version skew; anything else (wrong family stem, non-strings)
        // stays invalid too.
        const FAMILY_STEM: &str = "lekalo/task-links-integration/v";
        let version_skew = declared_version.is_some_and(|text| {
            text != version::SCHEMA_VERSION
                && text.strip_prefix(FAMILY_STEM).is_some_and(|rest| {
                    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == '.')
                })
        });
        if version_skew {
            return Err(diagnostic::input_invalid("schema-version-unsupported"));
        }
        return Err(diagnostic::input_invalid("identity"));
    }
    let string_member = |name: &str| -> Result<&str, diagnostic::Failure> {
        object
            .get(name)
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("member-shape"))
    };
    let attachment_revision = Digest::parse(string_member("attachmentRevision")?)
        .ok_or_else(|| diagnostic::input_invalid("revision-digest"))?;
    let project_id = string_member("projectId")?;
    if !is_identifier(project_id) {
        return Err(diagnostic::input_invalid("project-id"));
    }
    let model_ref_value = object
        .get("modelRef")
        .and_then(Json::as_object)
        .ok_or_else(|| diagnostic::input_invalid("model-ref-shape"))?;
    for key in model_ref_value.keys() {
        if !matches!(key.as_str(), "schemaVersion" | "irDigest") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    if model_ref_value.get("schemaVersion").and_then(Json::as_str) != Some("0.8.0") {
        return Err(diagnostic::input_invalid("model-version"));
    }
    let ir_digest_value = model_ref_value
        .get("irDigest")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("model-ref-shape"))?;
    let model_ref = ModelRef {
        model_version: "0.8.0".to_owned(),
        ir_digest: Digest::parse(ir_digest_value)
            .ok_or_else(|| diagnostic::input_invalid("model-ref-digest"))?,
    };
    let ir_ref = object
        .get("irRef")
        .and_then(Json::as_object)
        .ok_or_else(|| diagnostic::input_invalid("ir-ref-shape"))?;
    for key in ir_ref.keys() {
        if !matches!(key.as_str(), "identity" | "digest") {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    if ir_ref.get("identity").and_then(Json::as_str) != Some(super::version::SUCCESSOR_IR_IDENTITY)
    {
        return Err(diagnostic::input_invalid("ir-identity"));
    }
    let ir_digest = Digest::parse(
        ir_ref
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("ir-ref-shape"))?,
    )
    .ok_or_else(|| diagnostic::input_invalid("ir-digest"))?;
    if ir_digest != model_ref.ir_digest {
        return Err(diagnostic::input_invalid("ir-digest-mismatch"));
    }
    let owner_pins = owner_pins(
        object
            .get("ownerContractPins")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("pins-shape"))?,
    )?;
    let entities = entities(
        object
            .get("entities")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("entities-shape"))?,
    )?;
    let relations = relations(
        object
            .get("relations")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("relations-shape"))?,
    )?;
    let unique_constraints = unique_constraints(
        object
            .get("uniqueConstraints")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("constraints-shape"))?,
    )?;
    let provider_contracts = provider_contracts(
        object
            .get("providerContracts")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("contracts-shape"))?,
    )?;
    let capability_requirements = capability_requirements(
        object
            .get("linkCapabilityRequirements")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("capabilities-shape"))?,
    )?;
    let field_policies = field_policies(
        object
            .get("fieldPolicies")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("field-policies-shape"))?,
    )?;
    let action_policies = action_policies(
        object
            .get("actionPolicies")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("action-policies-shape"))?,
    )?;
    let operation_bindings = operation_bindings(
        object
            .get("operationBindings")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("bindings-shape"))?,
    )?;
    let delivery_rules = delivery_rules(
        object
            .get("deliveryRules")
            .and_then(Json::as_object)
            .ok_or_else(|| diagnostic::input_invalid("delivery-shape"))?,
    )?;
    let contributions = contributions(
        object
            .get("contributions")
            .and_then(Json::as_object)
            .ok_or_else(|| diagnostic::input_invalid("contributions-shape"))?,
    )?;
    Ok(Attachment {
        attachment_revision,
        project_id: project_id.to_owned(),
        model_ref,
        ir_digest,
        owner_pins,
        entities,
        relations,
        unique_constraints,
        provider_contracts,
        capability_requirements,
        field_policies,
        action_policies,
        operation_bindings,
        delivery_rules,
        contributions,
    })
}

/// One bounded `^[a-z][a-z0-9_]{0,62}$` identifier.
pub(crate) fn is_identifier(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    first.is_ascii_lowercase()
        && rest.len() <= 62
        && rest
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

/// One bounded qualified symbol (`module.symbol`, `module.symbol.part`,
/// or a four-part policy path `module.policy.group.name`).
pub(crate) fn is_qualified_symbol(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    (2..=4).contains(&parts.len())
        && parts.iter().all(|part| {
            let bytes = part.as_bytes();
            let Some((&first, rest)) = bytes.split_first() else {
                return false;
            };
            first.is_ascii_lowercase()
                && rest.len() <= 62
                && rest
                    .iter()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
        })
}

fn digest_member(value: &Json) -> Option<Digest> {
    Digest::parse(value.as_str()?)
}

fn owner_pins(array: &[Json]) -> Result<Vec<OwnerPin>, diagnostic::Failure> {
    if array.len() != 5 {
        return Err(diagnostic::input_invalid("pins-count"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("pin-shape"))?;
        for key in object.keys() {
            if !matches!(key.as_str(), "family" | "identity" | "digest") {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let family = object
            .get("family")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("pin-shape"))?;
        if !matches!(
            family,
            "authorization"
                | "transaction-concurrency"
                | "extended-effects"
                | "invariant-transition"
                | "storage-projection"
        ) {
            return Err(diagnostic::input_invalid("pin-family"));
        }
        let identity = object
            .get("identity")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("pin-shape"))?;
        let expected = match family {
            "storage-projection" => format!("dev.lekalo.{family}@0.4.0"),
            other => format!("dev.lekalo.{other}@0.2.16"),
        };
        if identity != expected {
            return Err(diagnostic::input_invalid("pin-identity"));
        }
        let digest = digest_member(
            object
                .get("digest")
                .ok_or_else(|| diagnostic::input_invalid("pin-shape"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("pin-digest"))?;
        parsed.push(OwnerPin {
            family: family.to_owned(),
            identity: identity.to_owned(),
            digest,
        });
    }
    parsed.sort_by(|left, right| left.family.cmp(&right.family));
    if has_adjacent_duplicate(&parsed, |pin| &pin.family) {
        return Err(diagnostic::input_invalid("duplicate-pin-family"));
    }
    Ok(parsed)
}

fn entities(array: &[Json]) -> Result<Vec<EntityBinding>, diagnostic::Failure> {
    if array.is_empty() || array.len() > 16 {
        return Err(diagnostic::input_invalid("entities-count"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("entity-shape"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "entity" | "identity" | "externalIdentity" | "description"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let entity = object
            .get("entity")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("entity-shape"))?;
        if !is_qualified_symbol(entity) {
            return Err(diagnostic::input_invalid("entity-symbol"));
        }
        let identity = bounded_token_array(
            object
                .get("identity")
                .and_then(Json::as_array)
                .ok_or_else(|| diagnostic::input_invalid("entity-shape"))?,
            8,
        )?;
        let external_identity = match object.get("externalIdentity") {
            None | Some(Json::Null) => false,
            Some(Json::Bool(flag)) => *flag,
            Some(_) => return Err(diagnostic::input_invalid("entity-shape")),
        };
        parsed.push(EntityBinding {
            entity: entity.to_owned(),
            identity,
            external_identity,
        });
    }
    parsed.sort_by(|left, right| left.entity.cmp(&right.entity));
    if has_adjacent_duplicate(&parsed, |entity| &entity.entity) {
        return Err(diagnostic::input_invalid("duplicate-entity"));
    }
    Ok(parsed)
}

fn relations(array: &[Json]) -> Result<Vec<Relation>, diagnostic::Failure> {
    if array.is_empty() || array.len() > version::MAX_RELATIONS {
        return Err(diagnostic::input_invalid("relations-count"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("relation-shape"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "id" | "target"
                    | "localFields"
                    | "targetFields"
                    | "min"
                    | "max"
                    | "ownership"
                    | "onDetach"
                    | "onDelete"
                    | "description"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let id = object
            .get("id")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("relation-shape"))?;
        if !is_qualified_symbol(id) {
            return Err(diagnostic::input_invalid("relation-symbol"));
        }
        let target = object
            .get("target")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("relation-shape"))?;
        if !is_qualified_symbol(target) {
            return Err(diagnostic::input_invalid("relation-target"));
        }
        let local_fields = bounded_token_array(
            object
                .get("localFields")
                .and_then(Json::as_array)
                .ok_or_else(|| diagnostic::input_invalid("relation-shape"))?,
            8,
        )?;
        let target_fields = bounded_token_array(
            object
                .get("targetFields")
                .and_then(Json::as_array)
                .ok_or_else(|| diagnostic::input_invalid("relation-shape"))?,
            8,
        )?;
        if local_fields.len() != target_fields.len() {
            return Err(diagnostic::input_invalid("relation-arity"));
        }
        let min = object
            .get("min")
            .and_then(Json::as_u64)
            .ok_or_else(|| diagnostic::input_invalid("relation-shape"))?;
        let max = match object.get("max") {
            // Runtime/schema parity: the schema requires the member, and
            // `null` is its explicit unbounded spelling. A missing member
            // is a shape violation, never a default.
            None => return Err(diagnostic::input_invalid("relation-shape")),
            Some(Json::Null) => None,
            Some(value) => Some(
                value
                    .as_u64()
                    .ok_or_else(|| diagnostic::input_invalid("relation-shape"))?,
            ),
        };
        if let Some(max) = max {
            if max < min {
                return Err(diagnostic::input_invalid("relation-cardinality"));
            }
        }
        for (key, allowed) in [
            ("ownership", &["local", "external", "shared"][..]),
            ("onDetach", &["retain-history", "forbid"][..]),
            ("onDelete", &["restrict", "detach"][..]),
        ] {
            let value = object
                .get(key)
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("relation-shape"))?;
            if !allowed.contains(&value) {
                return Err(diagnostic::input_invalid("relation-member"));
            }
        }
        parsed.push(Relation {
            id: id.to_owned(),
            target: target.to_owned(),
            local_fields,
            target_fields,
            min,
            max,
            ownership: object
                .get("ownership")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            on_detach: object
                .get("onDetach")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            on_delete: object
                .get("onDelete")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
        });
    }
    parsed.sort_by(|left, right| left.id.cmp(&right.id));
    if has_adjacent_duplicate(&parsed, |relation| &relation.id) {
        return Err(diagnostic::input_invalid("duplicate-relation"));
    }
    Ok(parsed)
}

fn unique_constraints(array: &[Json]) -> Result<Vec<UniqueConstraint>, diagnostic::Failure> {
    if array.is_empty() || array.len() > version::MAX_UNIQUE_CONSTRAINTS {
        return Err(diagnostic::input_invalid("constraints-count"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("constraint-shape"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "id" | "entity"
                    | "fields"
                    | "comparison"
                    | "normalizationRef"
                    | "scope"
                    | "description"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let id = object
            .get("id")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("constraint-shape"))?;
        if !is_qualified_symbol(id) {
            return Err(diagnostic::input_invalid("constraint-symbol"));
        }
        let entity = object
            .get("entity")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("constraint-shape"))?;
        if !is_qualified_symbol(entity) {
            return Err(diagnostic::input_invalid("constraint-entity"));
        }
        let fields = bounded_token_array(
            object
                .get("fields")
                .and_then(Json::as_array)
                .ok_or_else(|| diagnostic::input_invalid("constraint-shape"))?,
            8,
        )?;
        let comparison = object
            .get("comparison")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("constraint-shape"))?;
        if !matches!(comparison, "exact-text" | "declared-normalization") {
            return Err(diagnostic::input_invalid("constraint-comparison"));
        }
        let normalization_ref = match object.get("normalizationRef") {
            None | Some(Json::Null) => None,
            Some(value) => {
                let reference = value
                    .as_str()
                    .ok_or_else(|| diagnostic::input_invalid("constraint-shape"))?;
                if !is_qualified_symbol(reference) {
                    return Err(diagnostic::input_invalid("constraint-normalization"));
                }
                Some(reference.to_owned())
            }
        };
        if (comparison == "declared-normalization") != normalization_ref.is_some() {
            return Err(diagnostic::input_invalid("constraint-normalization"));
        }
        let scope = object
            .get("scope")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("constraint-shape"))?;
        if !matches!(
            scope,
            "workspace-installation-provider-object-external-id" | "declared"
        ) {
            return Err(diagnostic::input_invalid("constraint-scope"));
        }
        parsed.push(UniqueConstraint {
            id: id.to_owned(),
            entity: entity.to_owned(),
            fields,
            comparison: comparison.to_owned(),
            normalization_ref,
            scope: scope.to_owned(),
        });
    }
    parsed.sort_by(|left, right| left.id.cmp(&right.id));
    if has_adjacent_duplicate(&parsed, |constraint| &constraint.id) {
        return Err(diagnostic::input_invalid("duplicate-constraint"));
    }
    Ok(parsed)
}

fn provider_contracts(array: &[Json]) -> Result<Vec<ProviderContract>, diagnostic::Failure> {
    if array.is_empty() || array.len() > 8 {
        return Err(diagnostic::input_invalid("contracts-count"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("contract-shape"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "contractId"
                    | "providerId"
                    | "objectType"
                    | "upstreamPin"
                    | "revision"
                    | "requestSchemaRef"
                    | "responseSchemaRef"
                    | "errorSchemaRef"
                    | "revisionOrdering"
                    | "remoteDedup"
                    | "description"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let contract_id = object
            .get("contractId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("contract-shape"))?;
        if !is_qualified_symbol(contract_id) {
            return Err(diagnostic::input_invalid("contract-symbol"));
        }
        let revision = digest_member(
            object
                .get("revision")
                .ok_or_else(|| diagnostic::input_invalid("contract-shape"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("contract-revision"))?;
        for key in ["requestSchemaRef", "responseSchemaRef", "errorSchemaRef"] {
            let value = object
                .get(key)
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("contract-shape"))?;
            if !is_qualified_symbol(value) {
                return Err(diagnostic::input_invalid("contract-schema-ref"));
            }
        }
        for (key, allowed) in [
            (
                "revisionOrdering",
                &["comparable-total", "incomparable-declared"][..],
            ),
            (
                "remoteDedup",
                &["idempotency-key", "receipt-reconciliation", "none-declared"][..],
            ),
        ] {
            let value = object
                .get(key)
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("contract-shape"))?;
            if !allowed.contains(&value) {
                return Err(diagnostic::input_invalid("contract-member"));
            }
        }
        parsed.push(ProviderContract {
            contract_id: contract_id.to_owned(),
            provider_id: bounded_text(
                object
                    .get("providerId")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::input_invalid("contract-shape"))?,
            )?,
            object_type: bounded_text(
                object
                    .get("objectType")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::input_invalid("contract-shape"))?,
            )?,
            upstream_pin: upstream_pin(
                object
                    .get("upstreamPin")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::input_invalid("contract-shape"))?,
            )?,
            revision,
            request_schema_ref: object
                .get("requestSchemaRef")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            response_schema_ref: object
                .get("responseSchemaRef")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            error_schema_ref: object
                .get("errorSchemaRef")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            revision_ordering: object
                .get("revisionOrdering")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            remote_dedup: object
                .get("remoteDedup")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
        });
    }
    parsed.sort_by(|left, right| left.contract_id.cmp(&right.contract_id));
    if has_adjacent_duplicate(&parsed, |contract| &contract.contract_id) {
        return Err(diagnostic::input_invalid("duplicate-contract"));
    }
    Ok(parsed)
}

fn capability_requirements(
    array: &[Json],
) -> Result<Vec<CapabilityRequirement>, diagnostic::Failure> {
    if array.is_empty() || array.len() > 64 {
        return Err(diagnostic::input_invalid("capabilities-count"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("capability-shape"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "capabilityId" | "contractId" | "requirement" | "revision" | "description"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let capability_id = object
            .get("capabilityId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("capability-shape"))?;
        if !is_qualified_symbol(capability_id) {
            return Err(diagnostic::input_invalid("capability-symbol"));
        }
        let contract_id = object
            .get("contractId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("capability-shape"))?;
        if !is_qualified_symbol(contract_id) {
            return Err(diagnostic::input_invalid("capability-contract"));
        }
        let requirement = object
            .get("requirement")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("capability-shape"))?;
        if !matches!(requirement, "required" | "optional") {
            return Err(diagnostic::input_invalid("capability-requirement"));
        }
        let revision = digest_member(
            object
                .get("revision")
                .ok_or_else(|| diagnostic::input_invalid("capability-shape"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("capability-revision"))?;
        parsed.push(CapabilityRequirement {
            capability_id: capability_id.to_owned(),
            contract_id: contract_id.to_owned(),
            requirement: requirement.to_owned(),
            revision,
        });
    }
    parsed.sort_by(|left, right| {
        (left.capability_id.as_str(), left.contract_id.as_str())
            .cmp(&(right.capability_id.as_str(), right.contract_id.as_str()))
    });
    if parsed.windows(2).any(|window| {
        window[0].capability_id == window[1].capability_id
            && window[0].contract_id == window[1].contract_id
    }) {
        return Err(diagnostic::input_invalid("duplicate-capability"));
    }
    Ok(parsed)
}

fn field_policies(array: &[Json]) -> Result<Vec<FieldPolicy>, diagnostic::Failure> {
    if array.is_empty() || array.len() > 128 {
        return Err(diagnostic::input_invalid("field-policies-count"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("field-policy-shape"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "policyId"
                    | "entity"
                    | "direction"
                    | "decision"
                    | "fields"
                    | "actor"
                    | "policyRef"
                    | "revision"
                    | "description"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let policy_id = object
            .get("policyId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("field-policy-shape"))?;
        if !is_qualified_symbol(policy_id) {
            return Err(diagnostic::input_invalid("policy-symbol"));
        }
        for key in ["entity", "policyRef"] {
            let value = object
                .get(key)
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("field-policy-shape"))?;
            if !is_qualified_symbol(value) {
                return Err(diagnostic::input_invalid("policy-ref"));
            }
        }
        let direction = object
            .get("direction")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("field-policy-shape"))?;
        if !matches!(direction, "inbound" | "outbound") {
            return Err(diagnostic::input_invalid("policy-direction"));
        }
        let decision = object
            .get("decision")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("field-policy-shape"))?;
        if !matches!(decision, "allow" | "deny") {
            return Err(diagnostic::input_invalid("policy-decision"));
        }
        let actor = object
            .get("actor")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("field-policy-shape"))?;
        if !matches!(actor, "identity.user" | "system.job" | "provider.event") {
            return Err(diagnostic::input_invalid("policy-actor"));
        }
        let revision = digest_member(
            object
                .get("revision")
                .ok_or_else(|| diagnostic::input_invalid("field-policy-shape"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("policy-revision"))?;
        parsed.push(FieldPolicy {
            policy_id: policy_id.to_owned(),
            entity: object
                .get("entity")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            direction: direction.to_owned(),
            decision: decision.to_owned(),
            fields: bounded_token_array(
                object
                    .get("fields")
                    .and_then(Json::as_array)
                    .ok_or_else(|| diagnostic::input_invalid("field-policy-shape"))?,
                64,
            )?,
            actor: actor.to_owned(),
            policy_ref: object
                .get("policyRef")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            revision,
        });
    }
    parsed.sort_by(|left, right| left.policy_id.cmp(&right.policy_id));
    if has_adjacent_duplicate(&parsed, |policy| &policy.policy_id) {
        return Err(diagnostic::input_invalid("duplicate-field-policy"));
    }
    Ok(parsed)
}

fn action_policies(array: &[Json]) -> Result<Vec<ActionPolicy>, diagnostic::Failure> {
    if array.is_empty() || array.len() > 64 {
        return Err(diagnostic::input_invalid("action-policies-count"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("action-policy-shape"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "policyId"
                    | "action"
                    | "decision"
                    | "actor"
                    | "capabilityRef"
                    | "policyRef"
                    | "revision"
                    | "description"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        let policy_id = object
            .get("policyId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("action-policy-shape"))?;
        if !is_qualified_symbol(policy_id) {
            return Err(diagnostic::input_invalid("policy-symbol"));
        }
        let action = object
            .get("action")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("action-policy-shape"))?;
        if !is_qualified_symbol(action) {
            return Err(diagnostic::input_invalid("action-symbol"));
        }
        let decision = object
            .get("decision")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("action-policy-shape"))?;
        if !matches!(decision, "allow" | "deny") {
            return Err(diagnostic::input_invalid("policy-decision"));
        }
        let actor = object
            .get("actor")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("action-policy-shape"))?;
        if !matches!(actor, "identity.user" | "system.job" | "provider.event") {
            return Err(diagnostic::input_invalid("policy-actor"));
        }
        let revision = digest_member(
            object
                .get("revision")
                .ok_or_else(|| diagnostic::input_invalid("action-policy-shape"))?,
        )
        .ok_or_else(|| diagnostic::input_invalid("policy-revision"))?;
        parsed.push(ActionPolicy {
            policy_id: policy_id.to_owned(),
            action: action.to_owned(),
            decision: decision.to_owned(),
            actor: actor.to_owned(),
            capability_ref: match object.get("capabilityRef") {
                None | Some(Json::Null) => None,
                Some(value) => Some(
                    value
                        .as_str()
                        .ok_or_else(|| diagnostic::input_invalid("action-policy-shape"))?
                        .to_owned(),
                ),
            },
            policy_ref: object
                .get("policyRef")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("action-policy-shape"))?
                .to_owned(),
            revision,
        });
    }
    parsed.sort_by(|left, right| left.policy_id.cmp(&right.policy_id));
    if has_adjacent_duplicate(&parsed, |policy| &policy.policy_id) {
        return Err(diagnostic::input_invalid("duplicate-action-policy"));
    }
    Ok(parsed)
}

fn operation_bindings(array: &[Json]) -> Result<Vec<OperationBinding>, diagnostic::Failure> {
    if array.is_empty() || array.len() > 32 {
        return Err(diagnostic::input_invalid("bindings-count"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for entry in array {
        let object = entry
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("binding-shape"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "operation"
                    | "action"
                    | "capabilityRef"
                    | "actionPolicyRef"
                    | "effectRef"
                    | "classification"
                    | "timeoutMs"
                    | "retryGuarantee"
            ) {
                return Err(diagnostic::input_invalid("unknown-field"));
            }
        }
        for key in [
            "operation",
            "action",
            "capabilityRef",
            "actionPolicyRef",
            "effectRef",
        ] {
            let value = object
                .get(key)
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::input_invalid("binding-shape"))?;
            if !is_qualified_symbol(value) {
                return Err(diagnostic::input_invalid("binding-symbol"));
            }
        }
        let classification = object
            .get("classification")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("binding-shape"))?;
        if !matches!(classification, "public" | "internal") {
            return Err(diagnostic::input_invalid("binding-classification"));
        }
        let timeout_ms = object
            .get("timeoutMs")
            .and_then(Json::as_u64)
            .ok_or_else(|| diagnostic::input_invalid("binding-shape"))?;
        if timeout_ms == 0 || timeout_ms > 600_000 {
            return Err(diagnostic::input_invalid("binding-timeout"));
        }
        let retry_guarantee = object
            .get("retryGuarantee")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::input_invalid("binding-shape"))?;
        if !matches!(
            retry_guarantee,
            "idempotency-key" | "at-most-once" | "none-declared"
        ) {
            return Err(diagnostic::input_invalid("binding-retry"));
        }
        parsed.push(OperationBinding {
            operation: object
                .get("operation")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            action: object
                .get("action")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            capability_ref: object
                .get("capabilityRef")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            action_policy_ref: object
                .get("actionPolicyRef")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            effect_ref: object
                .get("effectRef")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            classification: classification.to_owned(),
            timeout_ms,
            retry_guarantee: retry_guarantee.to_owned(),
        });
    }
    parsed.sort_by(|left, right| left.operation.cmp(&right.operation));
    if has_adjacent_duplicate(&parsed, |binding| &binding.operation) {
        return Err(diagnostic::input_invalid("duplicate-binding"));
    }
    Ok(parsed)
}

fn delivery_rules(
    object: &serde_json::Map<String, Json>,
) -> Result<DeliveryRules, diagnostic::Failure> {
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "outboxStates"
                | "maxAttempts"
                | "backoffMs"
                | "leaseMs"
                | "networkInsideTransaction"
                | "externalDeleteImplicit"
                | "uncertaintyPolicy"
        ) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let states = object
        .get("outboxStates")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("delivery-shape"))?;
    let expected = [
        "pending",
        "leased",
        "accepted_pending_ack",
        "delivered",
        "retry_wait",
        "conflicted",
        "cancelled",
        "dead_letter",
        "unknown_delivery",
    ];
    if states.len() != expected.len()
        || states
            .iter()
            .zip(expected.iter())
            .any(|(value, expected)| value.as_str() != Some(*expected))
    {
        return Err(diagnostic::input_invalid("delivery-states"));
    }
    let positive = |key: &str| -> Result<u64, diagnostic::Failure> {
        let value = object
            .get(key)
            .and_then(Json::as_u64)
            .ok_or_else(|| diagnostic::input_invalid("delivery-shape"))?;
        Ok(value)
    };
    let max_attempts = positive("maxAttempts")?;
    if max_attempts == 0 || max_attempts > 64 {
        return Err(diagnostic::input_invalid("delivery-attempts"));
    }
    let backoff_ms = positive("backoffMs")?;
    if backoff_ms > 3_600_000 {
        return Err(diagnostic::input_invalid("delivery-backoff"));
    }
    let lease_ms = positive("leaseMs")?;
    if lease_ms == 0 || lease_ms > 3_600_000 {
        return Err(diagnostic::input_invalid("delivery-lease"));
    }
    if object.get("networkInsideTransaction") != Some(&Json::Bool(false)) {
        return Err(diagnostic::input_invalid("delivery-atomicity"));
    }
    if object.get("externalDeleteImplicit") != Some(&Json::Bool(false)) {
        return Err(diagnostic::input_invalid("delivery-delete"));
    }
    let uncertainty_policy = object
        .get("uncertaintyPolicy")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("delivery-shape"))?;
    if !matches!(
        uncertainty_policy,
        "report-unknown-reconcile" | "receipt-reconciliation"
    ) {
        return Err(diagnostic::input_invalid("delivery-uncertainty"));
    }
    Ok(DeliveryRules {
        outbox_states: expected.iter().map(|state| (*state).to_owned()).collect(),
        max_attempts,
        backoff_ms,
        lease_ms,
        network_inside_transaction: false,
        external_delete_implicit: false,
        uncertainty_policy: uncertainty_policy.to_owned(),
    })
}

fn contributions(
    object: &serde_json::Map<String, Json>,
) -> Result<Contributions, diagnostic::Failure> {
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "graph" | "impact" | "inspect" | "scenarios" | "effectTrace"
        ) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    let flag = |key: &str| -> Result<bool, diagnostic::Failure> {
        match object.get(key) {
            Some(Json::Bool(flag)) => Ok(*flag),
            _ => Err(diagnostic::input_invalid("contributions-shape")),
        }
    };
    Ok(Contributions {
        graph: flag("graph")?,
        impact: flag("impact")?,
        inspect: flag("inspect")?,
        scenarios: flag("scenarios")?,
        effect_trace: flag("effectTrace")?,
    })
}

fn bounded_text(text: &str) -> Result<String, diagnostic::Failure> {
    if text.is_empty() || text.len() > 256 {
        return Err(diagnostic::input_invalid("text-bound"));
    }
    Ok(text.to_owned())
}

fn upstream_pin(text: &str) -> Result<String, diagnostic::Failure> {
    if text.is_empty() || text.len() > 128 {
        return Err(diagnostic::input_invalid("pin-bound"));
    }
    let first = text.as_bytes()[0];
    if !(first.is_ascii_alphanumeric()) {
        return Err(diagnostic::input_invalid("pin-bound"));
    }
    if !text.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'/' | b'@' | b'-' | b'+')
    }) {
        return Err(diagnostic::input_invalid("pin-bound"));
    }
    Ok(text.to_owned())
}

fn bounded_token_array(array: &[Json], max: usize) -> Result<Vec<String>, diagnostic::Failure> {
    if array.is_empty() || array.len() > max {
        return Err(diagnostic::input_invalid("array-bound"));
    }
    let mut parsed = Vec::with_capacity(array.len());
    for value in array {
        let text = value
            .as_str()
            .ok_or_else(|| diagnostic::input_invalid("token-shape"))?;
        if text.is_empty() || text.len() > 256 {
            return Err(diagnostic::input_invalid("token-bound"));
        }
        parsed.push(text.to_owned());
    }
    Ok(parsed)
}

fn has_adjacent_duplicate<T, K: Ord>(sorted: &[T], key: impl Fn(&T) -> &K) -> bool {
    sorted
        .windows(2)
        .any(|window| key(&window[0]) == key(&window[1]))
}
