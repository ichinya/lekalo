//! Typed effect identities (issue #14).
//!
//! Every identity is a validated, path-independent newtype: resources carry
//! a closed kind plus either a canonical semantic id or a namespaced
//! adapter-owned id, fields are validated names inside an explicit
//! resource, and operations reuse the accepted #13 kind-qualified node
//! identity restricted to the `operation` kind. No raw unchecked string
//! enters through a public constructor.

use std::fmt;

/// The closed resource-kind registry entry.
///
/// `canonical` resources are semantic entities of the compiled project;
/// `event` resources are declared event symbols. Every other kind is
/// adapter-owned: it never pretends to be a Lekalo entity and never gains
/// canonical identity from a name.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum ResourceKind {
    /// A semantic entity of the compiled project.
    Canonical,
    /// A target resource owned by an adapter or target profile.
    TargetResource,
    /// An external service (an `external-call` subject).
    ExternalService,
    /// A cache resource (cache read/write/invalidate subjects).
    Cache,
    /// A declared or detected event symbol.
    Event,
    /// A job queue (an `enqueue-job` subject).
    Job,
    /// A published output (a `publish-output` subject).
    Output,
    /// An audit sink (an `audit-log` subject).
    Audit,
}

impl ResourceKind {
    /// The closed v1 kind keys in registry rank order.
    pub(crate) const KEYS: [Self; 8] = [
        Self::Canonical,
        Self::TargetResource,
        Self::ExternalService,
        Self::Cache,
        Self::Event,
        Self::Job,
        Self::Output,
        Self::Audit,
    ];

    /// The registry rank; the primary resource sort key.
    pub const fn rank(self) -> u8 {
        self as u8
    }

    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::TargetResource => "target-resource",
            Self::ExternalService => "external-service",
            Self::Cache => "cache",
            Self::Event => "event",
            Self::Job => "job",
            Self::Output => "output",
            Self::Audit => "audit",
        }
    }

    /// Registry lookup by exact key.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::KEYS
            .iter()
            .find(|candidate| candidate.key() == key)
            .copied()
    }

    /// Whether identities of this kind are canonical semantic ids.
    pub(crate) const fn is_semantic(self) -> bool {
        matches!(self, Self::Canonical | Self::Event)
    }
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.key())
    }
}

/// A typed, path-independent resource identity.
///
/// Semantic kinds (`canonical`, `event`) carry a node-safe semantic id;
/// every adapter-owned kind carries a namespaced id (`vendor.app/thing`)
/// and stays opaque. Construction is validated; the same IR and the same
/// evidence always produce the same identities.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct ResourceId {
    kind: ResourceKind,
    id: String,
}

impl ResourceId {
    /// Assemble one checked resource identity.
    pub fn new(kind: ResourceKind, id: &str) -> Option<Self> {
        if kind.is_semantic() {
            is_semantic_id(id).then(|| Self {
                kind,
                id: id.to_owned(),
            })
        } else {
            is_namespaced_id(id).then(|| Self {
                kind,
                id: id.to_owned(),
            })
        }
    }

    /// The resource kind.
    pub const fn kind(&self) -> ResourceKind {
        self.kind
    }

    /// The validated id below the kind.
    pub fn as_str(&self) -> &str {
        &self.id
    }
}

impl fmt::Display for ResourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.kind.key(), self.id)
    }
}

/// A validated field name inside one explicit resource.
///
/// Entity-level effects carry no field: absence is the explicit
/// entity-wide scope, never an empty sentinel, so an empty name is a
/// construction rejection.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct FieldName(String);

impl FieldName {
    /// Assemble one checked field name.
    pub fn new(name: &str) -> Option<Self> {
        let mut bytes = name.bytes();
        let valid = match bytes.next() {
            Some(first) => {
                first.is_ascii_lowercase() && name.len() <= 64 && bytes.all(is_name_byte)
            }
            None => false,
        };
        valid.then(|| Self(name.to_owned()))
    }

    /// The validated name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FieldName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The effect subject: one explicit resource plus its optional exact
/// field scope.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct Subject {
    resource: ResourceId,
    field: Option<FieldName>,
}

impl Subject {
    /// An entity-wide (resource-level) subject.
    pub fn new(resource: ResourceId) -> Self {
        Self {
            resource,
            field: None,
        }
    }

    /// A field-scoped subject.
    pub fn with_field(resource: ResourceId, field: FieldName) -> Self {
        Self {
            resource,
            field: Some(field),
        }
    }

    /// The subject resource.
    pub const fn resource(&self) -> &ResourceId {
        &self.resource
    }

    /// The exact field, when the effect is field-scoped.
    pub const fn field(&self) -> Option<&FieldName> {
        self.field.as_ref()
    }

    /// Whether the subject covers the whole resource (no field).
    pub const fn is_entity_wide(&self) -> bool {
        self.field.is_none()
    }
}

/// One qualified semantic operation (`operation:<semantic-id>`), reused
/// from the accepted #13 node identity.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct OperationId(String);

impl OperationId {
    /// Assemble one checked operation identity from a semantic id.
    pub fn from_semantic(semantic_id: &str) -> Option<Self> {
        crate::graph::NodeId::new(crate::graph::NodeKindId::OPERATION, semantic_id)
            .map(|node| Self(node.as_str().to_owned()))
    }

    /// Parse an exact `operation:<semantic-id>` wire form.
    pub fn from_qualified(qualified: &str) -> Option<Self> {
        let (kind, semantic_id) = qualified.split_once(':')?;
        (kind == "operation").then_some(())?;
        Self::from_semantic(semantic_id)
    }

    /// The bare semantic id.
    pub fn semantic_id(&self) -> &str {
        &self.0["operation:".len()..]
    }

    /// The exact wire form.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Where one effect edge's acting identity comes from: a declared effect
/// symbol of the compiled project, or a typed occurrence key supplied by
/// evidence without a declared symbol.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum EffectOrigin {
    /// A declared `effect` definition symbol.
    Declared(String),
    /// An evidence occurrence key; distinct occurrences stay distinct.
    Occurrence(u32),
}

impl EffectOrigin {
    /// The provenance-class sort rank (declared first).
    pub(crate) const fn rank(&self) -> u8 {
        match self {
            Self::Declared(_) => 0,
            Self::Occurrence(_) => 1,
        }
    }

    /// The canonical string form used by keys and diagnostics.
    pub fn to_canonical_string(&self) -> String {
        match self {
            Self::Declared(symbol) => format!("effect:{symbol}"),
            Self::Occurrence(ordinal) => format!("#{ordinal}"),
        }
    }
}

/// One explicit transaction-group reference carried on an effect edge.
///
/// The group is a descriptive boundary only: membership and overlap are
/// facts this graph reports; atomicity, isolation, and every other
/// transaction guarantee stay with #24.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct TransactionGroupId(String);

impl TransactionGroupId {
    /// Assemble one checked group id (namespaced grammar, 1-128 bytes).
    pub fn new(id: &str) -> Option<Self> {
        (!id.is_empty() && id.len() <= 128 && is_namespaced_id(id)).then(|| Self(id.to_owned()))
    }

    /// The validated group id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TransactionGroupId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A bounded, opaque sensitivity marker carried on an effect or subject.
///
/// The marker names a classification contract and its state; it never
/// defines actor, scope, authorization, or gate policy (#25 owns those)
/// and never carries field values or other private data.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct Sensitivity {
    contract: String,
    classified: bool,
}

impl Sensitivity {
    /// Assemble one checked marker: a namespaced contract id with an
    /// exact SemVer tail, plus the closed state.
    pub fn new(contract: &str, classified: bool) -> Option<Self> {
        is_provider_contract(contract).then(|| Self {
            contract: contract.to_owned(),
            classified,
        })
    }

    /// The opaque classification contract reference.
    pub fn contract(&self) -> &str {
        &self.contract
    }

    /// The closed state (`unclassified` / `classified`).
    pub const fn state(&self) -> &'static str {
        if self.classified {
            "classified"
        } else {
            "unclassified"
        }
    }

    /// Whether the marker is in the classified state.
    pub const fn classified(&self) -> bool {
        self.classified
    }
}

/// Whether `text` is a node-safe semantic id: the accepted #13 grammar
/// (non-empty, at most 192 bytes, ASCII alphanumeric plus `.` `-` `_` `:`).
pub(crate) fn is_semantic_id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 192
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
}

/// Canonical `major.minor.patch` without build metadata.
pub(crate) fn is_semver(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && text.len() <= 32
}
/// Whether `text` is a namespaced adapter-owned id: at most 64 bytes,
/// lowercase dot-separated names, then one `/` and the resource name —
/// exactly the accepted #13 extension-record grammar.
pub(crate) fn is_namespaced_id(text: &str) -> bool {
    if text.len() > 64 {
        return false;
    }
    match text.split_once('/') {
        Some((namespace, name)) => {
            is_lower_name_chain(namespace) && !name.is_empty() && is_lower_name(name)
        }
        None => false,
    }
}

/// Whether `text` is a namespaced classification contract: the namespaced
/// id grammar plus an exact SemVer tail after `@`.
pub(crate) fn is_provider_contract(text: &str) -> bool {
    match text.rsplit_once('@') {
        Some((namespace, version)) => is_namespaced_id(namespace) && is_semver(version),
        None => false,
    }
}

/// One lowercase name segment: letter first, letters/digits/hyphens after.
pub(crate) fn is_lower_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    match bytes.next() {
        Some(first) => {
            first.is_ascii_lowercase() && name.len() <= 32 && bytes.all(is_lower_name_byte)
        }
        None => false,
    }
}

fn is_lower_name_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

/// One lowercase dot-separated name chain (the namespace half).
fn is_lower_name_chain(namespace: &str) -> bool {
    !namespace.is_empty() && namespace.split('.').all(is_lower_name)
}
