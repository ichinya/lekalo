//! The typed classification-policy attachment (issue #87).
//!
//! The policy is the governance twin of the classification
//! attachment: per-kind reader/writer dimensions over the closed
//! authorization scope vocabulary, the closed destination set for
//! outbound flows, masking/retention/export references into the
//! #120 privacy family, encryption references into the #85 NFR
//! family, consent and cross-tenant mode, declassification roles,
//! and the non-model sink ceilings. The policy is metadata-only
//! declaration data and never restates referenced semantics.

use serde::Serialize;
use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::Sha256Digest;
use crate::scenario::id::SemanticId;

use super::diagnostic;
use super::types::{BoundedText, ContractRef, DataKind, PolicyRef, QuestionId};
use super::version;
use super::wire::{check_members, ir_pin, model_pin, semver};

/// The closed top-level member set of the policy.
const POLICY_TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "kinds",
    "sinks",
    "openQuestions",
];

/// The closed kind-rule member set.
const KIND_RULE_KEYS: &[&str] = &[
    "kind",
    "readers",
    "writers",
    "destinations",
    "masking",
    "retentionRef",
    "exportRef",
    "encryptionRefs",
    "consentRequired",
    "crossTenant",
    "declassifyRoles",
];

/// The closed masking member set.
const MASKING_KEYS: &[&str] = &["strategy", "policyRef"];

/// The closed sinks member set.
const SINKS_KEYS: &[&str] = &[
    "logs",
    "traces",
    "contextCapsules",
    "diagnostics",
    "evidence",
    "exports",
];

/// The closed sink-ceiling member set.
const SINK_CEILING_KEYS: &[&str] = &["maxKind"];

/// The closed policy open-question member set.
const POLICY_OPEN_QUESTION_KEYS: &[&str] = &["id", "question", "kind"];

/// The maximum number of reader/writer references one rule may carry.
pub const MAX_ACTORS: usize = 16;

/// The maximum number of destinations one rule may carry.
pub const MAX_DESTINATIONS: usize = 16;

/// The maximum number of encryption references one rule may carry.
pub const MAX_ENCRYPTION_REFS: usize = 8;

/// The maximum number of declassification roles one rule may carry.
pub const MAX_DECLASSIFY_ROLES: usize = 8;

/// The closed authorization scope dimension vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ScopeDimension {
    /// `tenant`.
    Tenant,
    /// `workspace`.
    Workspace,
    /// `user`.
    User,
}

impl ScopeDimension {
    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "tenant" => Self::Tenant,
            "workspace" => Self::Workspace,
            "user" => Self::User,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tenant => "tenant",
            Self::Workspace => "workspace",
            Self::User => "user",
        }
    }
}

/// One closed actor reference for read- or write-direction flows:
/// one scope dimension optionally narrowed by one scope token
/// (`dimension` or `dimension:scope`).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ActorRef(String);

impl ActorRef {
    /// Validate and keep the exact text of one actor reference.
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.is_empty() || bytes.len() > 64 + 1 + 64 {
            return None;
        }
        let (dimension, scope) = match text.split_once(':') {
            Some((dimension, scope)) => (dimension, Some(scope)),
            None => (text, None),
        };
        ScopeDimension::parse(dimension)?;
        if let Some(scope) = scope {
            let scope_bytes = scope.as_bytes();
            if scope_bytes.is_empty()
                || scope_bytes.len() > 64
                || !scope_bytes[0].is_ascii_lowercase()
            {
                return None;
            }
            if !scope_bytes[1..].iter().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
            }) {
                return None;
            }
        }
        Some(Self(text.to_owned()))
    }

    /// The dimension half of the reference.
    pub fn dimension(&self) -> ScopeDimension {
        ScopeDimension::parse(self.0.split(':').next().unwrap_or_default())
            .expect("dimension validated at parse")
    }

    /// The optional scope token.
    pub fn scope(&self) -> Option<&str> {
        self.0.split_once(':').map(|(_, scope)| scope)
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ActorRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The closed destination vocabulary (the extended-effects typed
/// destination surfaces).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DestinationKind {
    /// `internal-service`.
    InternalService,
    /// `partner-api`.
    PartnerApi,
    /// `public-webhook`.
    PublicWebhook,
    /// `message-bus`.
    MessageBus,
    /// `email`.
    Email,
    /// `storage-export`.
    StorageExport,
    /// `event-broker`.
    EventBroker,
}

impl DestinationKind {
    /// Every destination in canonical byte order.
    pub const ALL: [DestinationKind; 7] = [
        DestinationKind::Email,
        DestinationKind::EventBroker,
        DestinationKind::InternalService,
        DestinationKind::MessageBus,
        DestinationKind::PartnerApi,
        DestinationKind::PublicWebhook,
        DestinationKind::StorageExport,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "internal-service" => Self::InternalService,
            "partner-api" => Self::PartnerApi,
            "public-webhook" => Self::PublicWebhook,
            "message-bus" => Self::MessageBus,
            "email" => Self::Email,
            "storage-export" => Self::StorageExport,
            "event-broker" => Self::EventBroker,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InternalService => "internal-service",
            Self::PartnerApi => "partner-api",
            Self::PublicWebhook => "public-webhook",
            Self::MessageBus => "message-bus",
            Self::Email => "email",
            Self::StorageExport => "storage-export",
            Self::EventBroker => "event-broker",
        }
    }
}

impl Serialize for DestinationKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The closed masking-strategy vocabulary. #87 declares that masking
/// is required; #119 owns enforcement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum MaskingStrategy {
    /// `redact`.
    Redact,
    /// `hash`.
    Hash,
    /// `tokenize`.
    Tokenize,
    /// `suppress`.
    Suppress,
}

impl MaskingStrategy {
    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "redact" => Self::Redact,
            "hash" => Self::Hash,
            "tokenize" => Self::Tokenize,
            "suppress" => Self::Suppress,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Redact => "redact",
            Self::Hash => "hash",
            Self::Tokenize => "tokenize",
            Self::Suppress => "suppress",
        }
    }
}

impl Serialize for MaskingStrategy {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The closed cross-tenant mode: forbidden outright, or permitted only
/// with a reviewed grant.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum CrossTenantMode {
    /// `forbidden`.
    Forbidden,
    /// `reviewed`.
    Reviewed,
}

impl CrossTenantMode {
    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "forbidden" => Self::Forbidden,
            "reviewed" => Self::Reviewed,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Forbidden => "forbidden",
            Self::Reviewed => "reviewed",
        }
    }
}

impl Serialize for CrossTenantMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The closed non-model sink vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SinkName {
    /// `logs`.
    Logs,
    /// `traces`.
    Traces,
    /// `contextCapsules`.
    ContextCapsules,
    /// `diagnostics`.
    Diagnostics,
    /// `evidence`.
    Evidence,
    /// `exports`.
    Exports,
}

impl SinkName {
    /// Every sink in canonical declaration order.
    pub const ALL: [SinkName; 6] = [
        SinkName::Logs,
        SinkName::Traces,
        SinkName::ContextCapsules,
        SinkName::Diagnostics,
        SinkName::Evidence,
        SinkName::Exports,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "logs" => Self::Logs,
            "traces" => Self::Traces,
            "contextCapsules" => Self::ContextCapsules,
            "diagnostics" => Self::Diagnostics,
            "evidence" => Self::Evidence,
            "exports" => Self::Exports,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Logs => "logs",
            Self::Traces => "traces",
            Self::ContextCapsules => "contextCapsules",
            Self::Diagnostics => "diagnostics",
            Self::Evidence => "evidence",
            Self::Exports => "exports",
        }
    }
}

/// The declared masking requirement for one kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Masking {
    strategy: MaskingStrategy,
    policy_ref: PolicyRef,
}

impl Masking {
    /// The declared strategy.
    pub const fn strategy(&self) -> MaskingStrategy {
        self.strategy
    }

    /// The exact #120 policy document reference.
    pub fn policy_ref(&self) -> &PolicyRef {
        &self.policy_ref
    }
}

/// The policy for one classification kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KindRule {
    kind: DataKind,
    readers: Vec<ActorRef>,
    writers: Vec<ActorRef>,
    destinations: Vec<DestinationKind>,
    masking: Masking,
    retention_ref: Option<PolicyRef>,
    export_ref: Option<PolicyRef>,
    encryption_refs: Vec<ContractRef>,
    consent_required: bool,
    cross_tenant: CrossTenantMode,
    declassify_roles: Vec<String>,
}

impl KindRule {
    /// The governed kind.
    pub const fn kind(&self) -> DataKind {
        self.kind
    }

    /// The closed reader references, canonically ordered.
    pub fn readers(&self) -> &[ActorRef] {
        &self.readers
    }

    /// The closed writer references, canonically ordered.
    pub fn writers(&self) -> &[ActorRef] {
        &self.writers
    }

    /// The closed destination set, canonically ordered.
    pub fn destinations(&self) -> &[DestinationKind] {
        &self.destinations
    }

    /// The declared masking requirement.
    pub const fn masking(&self) -> &Masking {
        &self.masking
    }

    /// The optional retention reference (#120).
    pub fn retention_ref(&self) -> Option<&PolicyRef> {
        self.retention_ref.as_ref()
    }

    /// The optional export reference (#120).
    pub fn export_ref(&self) -> Option<&PolicyRef> {
        self.export_ref.as_ref()
    }

    /// The NFR constraint references (#85), canonically ordered.
    pub fn encryption_refs(&self) -> &[ContractRef] {
        &self.encryption_refs
    }

    /// Whether sinks consuming this kind require consent/approval.
    pub const fn consent_required(&self) -> bool {
        self.consent_required
    }

    /// The cross-tenant mode.
    pub const fn cross_tenant(&self) -> CrossTenantMode {
        self.cross_tenant
    }

    /// The closed declassification roles, canonically ordered. Empty
    /// for `credential`: secrets never declassify downward.
    pub fn declassify_roles(&self) -> &[String] {
        &self.declassify_roles
    }
}

/// The ceiling of one non-model sink.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SinkCeiling {
    max_kind: DataKind,
}

impl SinkCeiling {
    /// The highest kind this sink may consume.
    pub const fn max_kind(&self) -> DataKind {
        self.max_kind
    }
}

/// One policy open question.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyOpenQuestion {
    id: QuestionId,
    question: BoundedText,
    kind: Option<DataKind>,
}

/// The typed classification-policy attachment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyAttachment {
    attachment_revision: String,
    project_id: SemanticId,
    model_ref: (String, Sha256Digest),
    ir_ref: (String, Sha256Digest),
    kinds: Vec<KindRule>,
    sinks: Vec<(SinkName, SinkCeiling)>,
    open_questions: Vec<PolicyOpenQuestion>,
}

impl PolicyAttachment {
    /// Assemble from validated parts (wire internal).
    pub(crate) fn assemble(
        attachment_revision: String,
        project_id: SemanticId,
        model_ref: (String, Sha256Digest),
        ir_ref: (String, Sha256Digest),
        kinds: Vec<KindRule>,
        sinks: Vec<(SinkName, SinkCeiling)>,
        open_questions: Vec<PolicyOpenQuestion>,
    ) -> Self {
        Self {
            attachment_revision,
            project_id,
            model_ref,
            ir_ref,
            kinds,
            sinks,
            open_questions,
        }
    }

    /// Normalize one wire document into a validated policy, or return
    /// the typed rejection set.
    pub fn from_value(json: &Json) -> Result<Self, DiagnosticSet> {
        from_value(json)
    }

    /// Parse one policy document (exact UTF-8 JSON bytes) or return
    /// the terminal rejection set.
    pub fn parse(bytes: &[u8]) -> Result<Self, DiagnosticSet> {
        if bytes.len() > version::MAX_DOC_BYTES {
            return Err(diagnostic::document_invalid("document-bytes", None));
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|_| diagnostic::document_invalid("invalid-encoding", None))?;
        let json = super::json::parse(text)?;
        from_value(&json)
    }

    /// The exact attachment revision.
    pub fn attachment_revision(&self) -> &str {
        &self.attachment_revision
    }

    /// The stable project identity.
    pub fn project_id(&self) -> &SemanticId {
        &self.project_id
    }

    /// The bound Model pin `(modelVersion, digest)`.
    pub const fn model_ref(&self) -> &(String, Sha256Digest) {
        &self.model_ref
    }

    /// The bound IR pin `(irVersion, digest)`.
    pub const fn ir_ref(&self) -> &(String, Sha256Digest) {
        &self.ir_ref
    }

    /// Every kind rule, canonically ordered by kind rank.
    pub fn kinds(&self) -> &[KindRule] {
        &self.kinds
    }

    /// The rule for one kind, if declared.
    pub fn rule(&self, kind: DataKind) -> Option<&KindRule> {
        self.kinds.iter().find(|rule| rule.kind() == kind)
    }

    /// The ceiling of one non-model sink.
    pub fn sink(&self, sink: SinkName) -> Option<SinkCeiling> {
        self.sinks
            .iter()
            .find(|(name, _)| *name == sink)
            .map(|(_, ceiling)| *ceiling)
    }

    /// Every sink ceiling, canonically ordered by sink name.
    pub fn sinks(&self) -> &[(SinkName, SinkCeiling)] {
        &self.sinks
    }

    /// Every policy open question, canonically ordered by id.
    pub fn open_questions(&self) -> &[PolicyOpenQuestion] {
        &self.open_questions
    }
}

/// Normalize one wire document into a validated policy, or return the
/// typed rejection set with no partial attachment.
fn from_value(json: &Json) -> Result<PolicyAttachment, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("top-level-shape", None))?;
    check_members(object, POLICY_TOP_LEVEL_KEYS)?;
    if object.get("schemaVersion").and_then(Json::as_str)
        != Some(super::policy::policy_version::SCHEMA_VERSION)
    {
        return Err(diagnostic::document_invalid("schema-version", None));
    }
    if object.get("identity").and_then(Json::as_str)
        != Some(super::policy::policy_version::IDENTITY)
    {
        return Err(diagnostic::document_invalid("contract-identity", None));
    }
    let attachment_revision = semver(
        object
            .get("attachmentRevision")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("attachment-revision", None))?,
        "attachment-revision",
    )?;
    let project_id = SemanticId::parse_root(
        object
            .get("projectId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("project-id", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("project-id", None))?;
    let model_ref = model_pin(
        object
            .get("modelRef")
            .ok_or_else(|| diagnostic::document_invalid("model-ref", None))?,
    )?;
    let ir_ref = ir_pin(
        object
            .get("irRef")
            .ok_or_else(|| diagnostic::document_invalid("ir-ref", None))?,
    )?;
    let kinds = kind_rules(
        object
            .get("kinds")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::document_invalid("kind-list", None))?,
    )?;
    let sinks = sinks(
        object
            .get("sinks")
            .ok_or_else(|| diagnostic::document_invalid("sinks", None))?,
    )?;
    let open_questions = policy_open_questions(
        object
            .get("openQuestions")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::document_invalid("open-question-list", None))?,
    )?;
    Ok(PolicyAttachment::assemble(
        attachment_revision,
        project_id,
        model_ref,
        ir_ref,
        kinds,
        sinks,
        open_questions,
    ))
}

/// Parse every kind rule; sort canonically by kind rank and reject
/// duplicates.
fn kind_rules(json: &[Json]) -> Result<Vec<KindRule>, DiagnosticSet> {
    if json.is_empty() || json.len() > DataKind::ALL.len() {
        return Err(diagnostic::document_invalid("kind-list", None));
    }
    let mut parsed = Vec::with_capacity(json.len());
    for value in json {
        let object = value
            .as_object()
            .ok_or_else(|| diagnostic::document_invalid("kind-rule-shape", None))?;
        check_members(object, KIND_RULE_KEYS)?;
        let kind = DataKind::parse(
            object
                .get("kind")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("kind", None))?,
        )
        .ok_or_else(|| diagnostic::document_invalid("kind", None))?;
        let readers = actor_refs(object, "readers")?;
        let writers = actor_refs(object, "writers")?;
        let destinations =
            match object.get("destinations") {
                None => Vec::new(),
                Some(value) => {
                    let items = value
                        .as_array()
                        .ok_or_else(|| diagnostic::document_invalid("destinations", None))?;
                    if items.len() > MAX_DESTINATIONS {
                        return Err(diagnostic::document_invalid("destinations", None));
                    }
                    let mut parsed_destinations = Vec::with_capacity(items.len());
                    for item in items {
                        parsed_destinations.push(
                            DestinationKind::parse(item.as_str().ok_or_else(|| {
                                diagnostic::document_invalid("destinations", None)
                            })?)
                            .ok_or_else(|| diagnostic::document_invalid("destinations", None))?,
                        );
                    }
                    parsed_destinations.sort();
                    parsed_destinations.dedup();
                    parsed_destinations
                }
            };
        let masking_json = object
            .get("masking")
            .ok_or_else(|| diagnostic::document_invalid("masking", None))?;
        let masking_object = masking_json
            .as_object()
            .ok_or_else(|| diagnostic::document_invalid("masking", None))?;
        check_members(masking_object, MASKING_KEYS)?;
        let masking = Masking {
            strategy: MaskingStrategy::parse(
                masking_object
                    .get("strategy")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::document_invalid("masking", None))?,
            )
            .ok_or_else(|| diagnostic::document_invalid("masking", None))?,
            policy_ref: PolicyRef::parse(
                masking_object
                    .get("policyRef")
                    .and_then(Json::as_str)
                    .ok_or_else(|| diagnostic::document_invalid("masking", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("masking", None))?,
        };
        let retention_ref = optional_policy_ref(object, "retentionRef")?;
        let export_ref = optional_policy_ref(object, "exportRef")?;
        let encryption_refs = match object.get("encryptionRefs") {
            None => Vec::new(),
            Some(value) => {
                let items = value
                    .as_array()
                    .ok_or_else(|| diagnostic::document_invalid("encryption-refs", None))?;
                if items.len() > MAX_ENCRYPTION_REFS {
                    return Err(diagnostic::document_invalid("encryption-refs", None));
                }
                let mut parsed_refs = Vec::with_capacity(items.len());
                for item in items {
                    parsed_refs.push(
                        ContractRef::parse(item.as_str().ok_or_else(|| {
                            diagnostic::document_invalid("encryption-refs", None)
                        })?)
                        .map_err(|_| diagnostic::document_invalid("encryption-refs", None))?,
                    );
                }
                parsed_refs.sort();
                parsed_refs.dedup();
                parsed_refs
            }
        };
        let consent_required = match object.get("consentRequired") {
            None => false,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| diagnostic::document_invalid("consent-required", None))?,
        };
        let cross_tenant = match object.get("crossTenant") {
            None => {
                if kind.cross_tenant_forbidden_by_default() {
                    CrossTenantMode::Forbidden
                } else {
                    CrossTenantMode::Reviewed
                }
            }
            Some(value) => CrossTenantMode::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("cross-tenant", None))?,
            )
            .ok_or_else(|| diagnostic::document_invalid("cross-tenant", None))?,
        };
        let declassify_roles = match object.get("declassifyRoles") {
            None => Vec::new(),
            Some(value) => {
                let items = value
                    .as_array()
                    .ok_or_else(|| diagnostic::document_invalid("declassify-roles", None))?;
                if items.len() > MAX_DECLASSIFY_ROLES {
                    return Err(diagnostic::document_invalid("declassify-roles", None));
                }
                let mut parsed_roles = Vec::with_capacity(items.len());
                for item in items {
                    let role = item
                        .as_str()
                        .ok_or_else(|| diagnostic::document_invalid("declassify-roles", None))?;
                    let role_bytes = role.as_bytes();
                    if role_bytes.is_empty()
                        || role_bytes.len() > 64
                        || !role_bytes[0].is_ascii_lowercase()
                        || !role_bytes[1..].iter().all(|byte| {
                            byte.is_ascii_lowercase()
                                || byte.is_ascii_digit()
                                || matches!(byte, b'_' | b'-')
                        })
                    {
                        return Err(diagnostic::document_invalid("declassify-roles", None));
                    }
                    parsed_roles.push(role.to_owned());
                }
                parsed_roles.sort();
                parsed_roles.dedup();
                parsed_roles
            }
        };
        parsed.push(KindRule {
            kind,
            readers,
            writers,
            destinations,
            masking,
            retention_ref,
            export_ref,
            encryption_refs,
            consent_required,
            cross_tenant,
            declassify_roles,
        });
    }
    parsed.sort_by_key(|rule| rule.kind().rank());
    for pair in parsed.windows(2) {
        if pair[0].kind() == pair[1].kind() {
            return Err(diagnostic::document_invalid("duplicate-kind-rule", None));
        }
    }
    // Hard rule: credential rules list no destinations and no
    // declassification roles; secrets never declassify downward.
    if let Some(credential) = parsed
        .iter()
        .find(|rule| rule.kind() == DataKind::Credential)
    {
        if !credential.destinations().is_empty() {
            return Err(diagnostic::document_invalid(
                "credential-destinations",
                None,
            ));
        }
        if !credential.declassify_roles().is_empty() {
            return Err(diagnostic::document_invalid(
                "credential-declassify-roles",
                None,
            ));
        }
    }
    Ok(parsed)
}

/// Parse one closed actor-reference list.
fn actor_refs(
    object: &serde_json::Map<String, Json>,
    key: &str,
) -> Result<Vec<ActorRef>, DiagnosticSet> {
    let Some(value) = object.get(key) else {
        return Err(diagnostic::document_invalid("actor-list", None));
    };
    let items = value
        .as_array()
        .ok_or_else(|| diagnostic::document_invalid("actor-list", None))?;
    if items.len() > MAX_ACTORS {
        return Err(diagnostic::document_invalid("actor-list", None));
    }
    let mut parsed = Vec::with_capacity(items.len());
    for item in items {
        let text = item
            .as_str()
            .ok_or_else(|| diagnostic::document_invalid("actor-list", None))?;
        parsed.push(
            ActorRef::parse(text)
                .ok_or_else(|| diagnostic::document_invalid("actor-list", None))?,
        );
    }
    parsed.sort();
    parsed.dedup();
    Ok(parsed)
}

/// Parse one optional policy reference member.
fn optional_policy_ref(
    object: &serde_json::Map<String, Json>,
    key: &str,
) -> Result<Option<PolicyRef>, DiagnosticSet> {
    match object.get(key) {
        None => Ok(None),
        Some(value) => Ok(Some(
            PolicyRef::parse(
                value
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("policy-ref", None))?,
            )
            .map_err(|_| diagnostic::document_invalid("policy-ref", None))?,
        )),
    }
}

/// Parse the closed sinks block: exactly six members, each a ceiling.
fn sinks(json: &Json) -> Result<Vec<(SinkName, SinkCeiling)>, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("sinks", None))?;
    check_members(object, SINKS_KEYS)?;
    let mut parsed = Vec::with_capacity(SinkName::ALL.len());
    for sink in SinkName::ALL {
        let value = object
            .get(sink.as_str())
            .ok_or_else(|| diagnostic::document_invalid("sinks", None))?;
        let ceiling = value
            .as_object()
            .ok_or_else(|| diagnostic::document_invalid("sink-ceiling", None))?;
        check_members(ceiling, SINK_CEILING_KEYS)?;
        let max_kind = DataKind::parse(
            ceiling
                .get("maxKind")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("sink-ceiling", None))?,
        )
        .ok_or_else(|| diagnostic::document_invalid("sink-ceiling", None))?;
        parsed.push((sink, SinkCeiling { max_kind }));
    }
    Ok(parsed)
}

/// Parse every policy open question; sort canonically by id.
fn policy_open_questions(json: &[Json]) -> Result<Vec<PolicyOpenQuestion>, DiagnosticSet> {
    if json.len() > version::MAX_OPEN_QUESTIONS {
        return Err(diagnostic::document_invalid("open-question-list", None));
    }
    let mut parsed = Vec::with_capacity(json.len());
    for value in json {
        let object = value
            .as_object()
            .ok_or_else(|| diagnostic::document_invalid("open-question-shape", None))?;
        check_members(object, POLICY_OPEN_QUESTION_KEYS)?;
        let id = QuestionId::parse(
            object
                .get("id")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("question-id", None))?,
        )
        .map_err(|_| diagnostic::document_invalid("question-id", None))?;
        let question = BoundedText::parse(
            object
                .get("question")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("question", None))?,
        )
        .map_err(|_| diagnostic::document_invalid("question", None))?;
        let kind = match object.get("kind") {
            None => None,
            Some(value) => Some(
                DataKind::parse(
                    value
                        .as_str()
                        .ok_or_else(|| diagnostic::document_invalid("kind", None))?,
                )
                .ok_or_else(|| diagnostic::document_invalid("kind", None))?,
            ),
        };
        parsed.push(PolicyOpenQuestion { id, question, kind });
    }
    parsed.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
    for pair in parsed.windows(2) {
        if pair[0].id().as_str() == pair[1].id().as_str() {
            return Err(diagnostic::document_invalid("duplicate-question", None));
        }
    }
    Ok(parsed)
}

/// The canonical wire view of the policy: borrowed strings in the
/// closed member set, serialized with byte-sorted object keys by the
/// canonical writer.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PolicyWire<'a> {
    schema_version: &'static str,
    identity: &'static str,
    attachment_revision: &'a str,
    project_id: &'a str,
    model_ref: super::wire::ModelRefWire<'a>,
    ir_ref: super::wire::IrRefWire<'a>,
    kinds: Vec<KindRuleWire<'a>>,
    sinks: SinksWire<'a>,
    open_questions: Vec<PolicyOpenQuestionWire<'a>>,
}

/// The canonical wire view of one kind rule.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KindRuleWire<'a> {
    kind: &'a str,
    readers: Vec<&'a str>,
    writers: Vec<&'a str>,
    destinations: Vec<&'static str>,
    masking: MaskingWire<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retention_ref: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    export_ref: Option<&'a str>,
    encryption_refs: Vec<&'a str>,
    consent_required: bool,
    cross_tenant: &'static str,
    declassify_roles: Vec<&'a str>,
}

/// The canonical wire view of the masking declaration.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MaskingWire<'a> {
    strategy: &'static str,
    policy_ref: &'a str,
}

/// The canonical wire view of the sinks block.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SinksWire<'a> {
    logs: CeilingWire<'a>,
    traces: CeilingWire<'a>,
    context_capsules: CeilingWire<'a>,
    diagnostics: CeilingWire<'a>,
    evidence: CeilingWire<'a>,
    exports: CeilingWire<'a>,
}

/// The canonical wire view of one sink ceiling.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CeilingWire<'a> {
    max_kind: &'a str,
}

/// The canonical wire view of one policy open question.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PolicyOpenQuestionWire<'a> {
    id: &'a str,
    question: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'a str>,
}

impl PolicyOpenQuestion {
    /// The question id.
    pub fn id(&self) -> &QuestionId {
        &self.id
    }

    /// The bounded question text.
    pub fn question(&self) -> &BoundedText {
        &self.question
    }

    /// The optional related kind.
    pub const fn kind(&self) -> Option<DataKind> {
        self.kind
    }
}

/// The closed policy family constants, re-exported for wire checks.
pub(super) mod policy_version {
    /// The policy family identifier.
    #[allow(dead_code)]
    pub const FAMILY: &str = "dev.lekalo.classification-policy";
    /// The exact policy contract version.
    #[allow(dead_code)]
    pub const VERSION: &str = "0.4.0";
    #[allow(dead_code)]
    pub const IDENTITY: &str = "dev.lekalo.classification-policy@0.4.0";
    pub const SCHEMA_VERSION: &str = "lekalo/classification-policy/v0.4.0";

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn identity_parts_are_consistent() {
            assert_eq!(IDENTITY, format!("{FAMILY}@{VERSION}"));
            assert!(SCHEMA_VERSION.ends_with(VERSION));
        }
    }
}

impl PolicyAttachment {
    /// The canonical borrowed wire view (canonical internal).
    pub(crate) fn wire(&self) -> PolicyWire<'_> {
        let ceiling = |sink: SinkName| -> CeilingWire<'static> {
            CeilingWire {
                max_kind: self
                    .sink(sink)
                    .expect("sinks close over every sink")
                    .max_kind()
                    .as_str(),
            }
        };
        PolicyWire {
            schema_version: policy_version::SCHEMA_VERSION,
            identity: policy_version::IDENTITY,
            attachment_revision: &self.attachment_revision,
            project_id: self.project_id.as_str(),
            model_ref: super::wire::ModelRefWire {
                model_version: &self.model_ref.0,
                digest: self.model_ref.1.as_str(),
            },
            ir_ref: super::wire::IrRefWire {
                ir_version: &self.ir_ref.0,
                digest: self.ir_ref.1.as_str(),
            },
            kinds: self
                .kinds
                .iter()
                .map(|rule| KindRuleWire {
                    kind: rule.kind().as_str(),
                    readers: rule.readers().iter().map(|actor| actor.as_str()).collect(),
                    writers: rule.writers().iter().map(|actor| actor.as_str()).collect(),
                    destinations: rule
                        .destinations()
                        .iter()
                        .map(|destination| destination.as_str())
                        .collect(),
                    masking: MaskingWire {
                        strategy: rule.masking().strategy().as_str(),
                        policy_ref: rule.masking().policy_ref().as_str(),
                    },
                    retention_ref: rule.retention_ref().map(PolicyRef::as_str),
                    export_ref: rule.export_ref().map(PolicyRef::as_str),
                    encryption_refs: rule
                        .encryption_refs()
                        .iter()
                        .map(|contract| contract.as_str())
                        .collect(),
                    consent_required: rule.consent_required(),
                    cross_tenant: rule.cross_tenant().as_str(),
                    declassify_roles: rule.declassify_roles().iter().map(String::as_str).collect(),
                })
                .collect(),
            sinks: SinksWire {
                logs: ceiling(SinkName::Logs),
                traces: ceiling(SinkName::Traces),
                context_capsules: ceiling(SinkName::ContextCapsules),
                diagnostics: ceiling(SinkName::Diagnostics),
                evidence: ceiling(SinkName::Evidence),
                exports: ceiling(SinkName::Exports),
            },
            open_questions: self
                .open_questions
                .iter()
                .map(|question| PolicyOpenQuestionWire {
                    id: question.id().as_str(),
                    question: question.question().as_str(),
                    kind: question.kind().map(DataKind::as_str),
                })
                .collect(),
        }
    }
}
