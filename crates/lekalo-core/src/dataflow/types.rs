//! The closed row vocabulary of the data-flow report (issue #87).
//!
//! Every member is bounded, closed, and metadata-only: findings
//! reference subjects by path and carry fixed detail tokens; flows
//! carry kinds, confidence, provenance, tenant relation, and the gate
//! state. No member can carry a value, a secret, a path on disk, or a
//! runtime principal.

use serde::Serialize;
use serde_json::Value as Json;

use super::version;

/// Why one textual member is not a valid closed-vocabulary value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VocabularyError {
    /// The text is empty or longer than the closed bound.
    Length,
    /// The text violates the closed spelling.
    Shape,
}

/// The subject-path grammar re-exported from the classification
/// family: one semantic id plus zero to two `/`-separated field
/// segments.
pub type SubjectPath = super::super::classification::types::SubjectPath;

/// The closed confidence vocabulary (the impact-analyzer terms).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Confidence {
    /// `high`.
    High,
    /// `low`.
    Low,
    /// `unknown`.
    Unknown,
}

impl Confidence {
    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "high" => Self::High,
            "low" => Self::Low,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Low => "low",
            Self::Unknown => "unknown",
        }
    }
}

impl Serialize for Confidence {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The closed flow-provenance vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Provenance {
    /// `canonical` — derived from the closed Model/IR.
    Canonical,
    /// `observed` — derived from recorded evidence.
    Observed,
    /// `declared` — declared-only, unverified.
    Declared,
}

impl Provenance {
    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "canonical" => Self::Canonical,
            "observed" => Self::Observed,
            "declared" => Self::Declared,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::Observed => "observed",
            Self::Declared => "declared",
        }
    }
}

impl Serialize for Provenance {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The closed tenant-relation vocabulary. `unknown` is treated as
/// crossing by the gate evaluation (plan §4.4).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum TenantRelation {
    /// `same`.
    Same,
    /// `crossing`.
    Crossing,
    /// `unknown`.
    Unknown,
}

impl TenantRelation {
    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "same" => Self::Same,
            "crossing" => Self::Crossing,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Same => "same",
            Self::Crossing => "crossing",
            Self::Unknown => "unknown",
        }
    }
}

impl Serialize for TenantRelation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The closed sink vocabulary of the data-flow projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SinkKind {
    /// `storage-write`.
    StorageWrite,
    /// `cache-write`.
    CacheWrite,
    /// `event-publish`.
    EventPublish,
    /// `external-call`.
    ExternalCall,
    /// `publication`.
    Publication,
    /// `endpoint-response`.
    EndpointResponse,
    /// `log`.
    Log,
    /// `trace`.
    Trace,
    /// `context-capsule`.
    ContextCapsule,
    /// `diagnostic`.
    Diagnostic,
    /// `evidence`.
    Evidence,
    /// `export`.
    Export,
}

impl SinkKind {
    /// Every sink kind in canonical byte order.
    pub const ALL: [SinkKind; 12] = [
        SinkKind::CacheWrite,
        SinkKind::ContextCapsule,
        SinkKind::Diagnostic,
        SinkKind::EndpointResponse,
        SinkKind::Evidence,
        SinkKind::EventPublish,
        SinkKind::Export,
        SinkKind::ExternalCall,
        SinkKind::Log,
        SinkKind::Publication,
        SinkKind::StorageWrite,
        SinkKind::Trace,
    ];

    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "storage-write" => Self::StorageWrite,
            "cache-write" => Self::CacheWrite,
            "event-publish" => Self::EventPublish,
            "external-call" => Self::ExternalCall,
            "publication" => Self::Publication,
            "endpoint-response" => Self::EndpointResponse,
            "log" => Self::Log,
            "trace" => Self::Trace,
            "context-capsule" => Self::ContextCapsule,
            "diagnostic" => Self::Diagnostic,
            "evidence" => Self::Evidence,
            "export" => Self::Export,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StorageWrite => "storage-write",
            Self::CacheWrite => "cache-write",
            Self::EventPublish => "event-publish",
            Self::ExternalCall => "external-call",
            Self::Publication => "publication",
            Self::EndpointResponse => "endpoint-response",
            Self::Log => "log",
            Self::Trace => "trace",
            Self::ContextCapsule => "context-capsule",
            Self::Diagnostic => "diagnostic",
            Self::Evidence => "evidence",
            Self::Export => "export",
        }
    }

    /// Whether the sink is one of the non-model policy surfaces.
    pub const fn is_policy_sink(self) -> bool {
        matches!(
            self,
            Self::Log
                | Self::Trace
                | Self::ContextCapsule
                | Self::Diagnostic
                | Self::Evidence
                | Self::Export
        )
    }

    /// Whether the sink is a gated surface: the plan §3.2 sinks whose
    /// destination, approval, and consent rules are evaluated by the
    /// analyzer (external-call, publication, public-endpoint response,
    /// export, cache).
    pub const fn is_gated(self) -> bool {
        matches!(
            self,
            Self::ExternalCall | Self::Publication | Self::CacheWrite | Self::Export
        )
    }
}

impl Serialize for SinkKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The closed data-kind vocabulary, re-exported from the
/// classification family.
pub type DataKind = super::super::classification::types::DataKind;

/// The closed gate-state block of one flow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gate {
    pub(crate) required: bool,
    pub(crate) satisfied: bool,
    pub(crate) reason: GateReason,
}

impl Gate {
    /// Whether a gate decision was required at all.
    pub const fn required(&self) -> bool {
        self.required
    }

    /// Whether the gate was satisfied.
    pub const fn satisfied(&self) -> bool {
        self.satisfied
    }

    /// The closed reason for the gate state.
    pub const fn reason(&self) -> GateReason {
        self.reason
    }

    /// The canonical wire form.
    pub(crate) fn to_wire(&self) -> Json {
        serde_json::json!({
            "required": self.required,
            "satisfied": self.satisfied,
            "reason": self.reason.as_str(),
        })
    }

    /// Parse the wire form.
    pub(crate) fn from_wire(json: &Json) -> Result<Self, VocabularyError> {
        let object = json.as_object().ok_or(VocabularyError::Shape)?;
        let required = object
            .get("required")
            .and_then(Json::as_bool)
            .ok_or(VocabularyError::Shape)?;
        let satisfied = object
            .get("satisfied")
            .and_then(Json::as_bool)
            .ok_or(VocabularyError::Shape)?;
        let reason = GateReason::parse(
            object
                .get("reason")
                .and_then(Json::as_str)
                .ok_or(VocabularyError::Shape)?,
        )
        .ok_or(VocabularyError::Shape)?;
        Ok(Self {
            required,
            satisfied,
            reason,
        })
    }
}

/// The closed gate-reason vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum GateReason {
    /// `not-required`.
    NotRequired,
    /// `destination-declared`.
    DestinationDeclared,
    /// `approval-present`.
    ApprovalPresent,
    /// `missing-destination`.
    MissingDestination,
    /// `missing-approval`.
    MissingApproval,
    /// `destination-forbidden`.
    DestinationForbidden,
    /// `unknown-flow`.
    UnknownFlow,
    /// `sink-ceiling-exceeded`.
    SinkCeilingExceeded,
    /// `unclassified-subject`.
    UnclassifiedSubject,
}

impl GateReason {
    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "not-required" => Self::NotRequired,
            "destination-declared" => Self::DestinationDeclared,
            "approval-present" => Self::ApprovalPresent,
            "missing-destination" => Self::MissingDestination,
            "missing-approval" => Self::MissingApproval,
            "destination-forbidden" => Self::DestinationForbidden,
            "unknown-flow" => Self::UnknownFlow,
            "low-confidence" => Self::LowConfidence,
            "sink-ceiling-exceeded" => Self::SinkCeilingExceeded,
            "unclassified-subject" => Self::UnclassifiedSubject,
            "inputs-incomplete" => Self::InputsIncomplete,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotRequired => "not-required",
            Self::DestinationDeclared => "destination-declared",
            Self::ApprovalPresent => "approval-present",
            Self::MissingDestination => "missing-destination",
            Self::MissingApproval => "missing-approval",
            Self::DestinationForbidden => "destination-forbidden",
            Self::UnknownFlow => "unknown-flow",
            Self::LowConfidence => "low-confidence",
            Self::SinkCeilingExceeded => "sink-ceiling-exceeded",
            Self::UnclassifiedSubject => "unclassified-subject",
            Self::InputsIncomplete => "inputs-incomplete",
        }
    }
}

impl Serialize for GateReason {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The gate-state alias used by the report builder.
pub type GateState = Gate;

/// One flow: source subject, ordered path, sink, and the resolved
/// classification with its gate decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Flow {
    pub(crate) id: String,
    pub(crate) source: SubjectPath,
    pub(crate) path: Vec<String>,
    pub(crate) sink: String,
    pub(crate) sink_kind: SinkKind,
    pub(crate) provenance: Provenance,
    pub(crate) confidence: Confidence,
    pub(crate) tenant_relation: TenantRelation,
    pub(crate) classification: DataKind,
    pub(crate) gate: Option<Gate>,
}

impl Flow {
    /// The flow id (builder-assigned, canonical order).
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The source subject.
    pub fn source(&self) -> &SubjectPath {
        &self.source
    }

    /// The ordered opaque edge references.
    pub fn path(&self) -> &[String] {
        &self.path
    }

    /// The sink symbol or bounded sink token.
    pub fn sink(&self) -> &str {
        &self.sink
    }

    /// The closed sink kind.
    pub const fn sink_kind(&self) -> SinkKind {
        self.sink_kind
    }

    /// The flow provenance.
    pub const fn provenance(&self) -> Provenance {
        self.provenance
    }

    /// The flow confidence.
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    /// The tenant relation.
    pub const fn tenant_relation(&self) -> TenantRelation {
        self.tenant_relation
    }

    /// The resolved classification of the source subject.
    pub const fn classification(&self) -> DataKind {
        self.classification
    }

    /// The gate decision, when the sink is a gated kind.
    pub fn gate(&self) -> Option<&Gate> {
        self.gate.as_ref()
    }

    /// The canonical wire form.
    pub(crate) fn to_wire(&self) -> Json {
        let mut wire = serde_json::json!({
            "id": self.id,
            "source": self.source.as_str(),
            "path": self.path,
            "sink": self.sink,
            "sinkKind": self.sink_kind.as_str(),
            "provenance": self.provenance.as_str(),
            "confidence": self.confidence.as_str(),
            "tenantRelation": self.tenant_relation.as_str(),
            "classification": self.classification.as_str(),
        });
        if let Some(gate) = &self.gate {
            wire.as_object_mut()
                .expect("flow wire object")
                .insert("gate".to_owned(), gate.to_wire());
        }
        wire
    }

    /// Parse the wire form (row-level validation).
    pub(crate) fn from_wire(json: &Json) -> Result<Self, VocabularyError> {
        let object = json.as_object().ok_or(VocabularyError::Shape)?;
        let id = object
            .get("id")
            .and_then(Json::as_str)
            .ok_or(VocabularyError::Shape)?;
        if id.is_empty() || id.len() > 64 {
            return Err(VocabularyError::Length);
        }
        let source = SubjectPath::parse(
            object
                .get("source")
                .and_then(Json::as_str)
                .ok_or(VocabularyError::Shape)?,
        )
        .map_err(|_| VocabularyError::Shape)?;
        let path_json = object
            .get("path")
            .and_then(Json::as_array)
            .ok_or(VocabularyError::Shape)?;
        if path_json.len() > version::MAX_PATH_HOPS {
            return Err(VocabularyError::Length);
        }
        let mut path = Vec::with_capacity(path_json.len());
        for hop in path_json {
            let hop = hop.as_str().ok_or(VocabularyError::Shape)?;
            if hop.is_empty() || hop.len() > 192 {
                return Err(VocabularyError::Length);
            }
            path.push(hop.to_owned());
        }
        let sink = object
            .get("sink")
            .and_then(Json::as_str)
            .ok_or(VocabularyError::Shape)?;
        if sink.is_empty() || sink.len() > 192 {
            return Err(VocabularyError::Length);
        }
        let sink_kind = SinkKind::parse(
            object
                .get("sinkKind")
                .and_then(Json::as_str)
                .ok_or(VocabularyError::Shape)?,
        )
        .ok_or(VocabularyError::Shape)?;
        let provenance = Provenance::parse(
            object
                .get("provenance")
                .and_then(Json::as_str)
                .ok_or(VocabularyError::Shape)?,
        )
        .ok_or(VocabularyError::Shape)?;
        let confidence = Confidence::parse(
            object
                .get("confidence")
                .and_then(Json::as_str)
                .ok_or(VocabularyError::Shape)?,
        )
        .ok_or(VocabularyError::Shape)?;
        let tenant_relation = TenantRelation::parse(
            object
                .get("tenantRelation")
                .and_then(Json::as_str)
                .ok_or(VocabularyError::Shape)?,
        )
        .ok_or(VocabularyError::Shape)?;
        let classification = DataKind::parse(
            object
                .get("classification")
                .and_then(Json::as_str)
                .ok_or(VocabularyError::Shape)?,
        )
        .ok_or(VocabularyError::Shape)?;
        let gate = match object.get("gate") {
            None => None,
            Some(gate) => Some(Gate::from_wire(gate)?),
        };
        Ok(Self {
            id: id.to_owned(),
            source,
            path,
            sink: sink.to_owned(),
            sink_kind,
            provenance,
            confidence,
            tenant_relation,
            classification,
            gate,
        })
    }
}

/// The closed finding-severity vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Severity {
    /// `error`.
    Error,
    /// `warning`.
    Warning,
}

impl Severity {
    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "error" => Self::Error,
            "warning" => Self::Warning,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }

    /// Whether the severity blocks the verdict.
    pub const fn is_error(self) -> bool {
        matches!(self, Self::Error)
    }
}

impl Serialize for Severity {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// One finding: a registered `classification.*`/`dataflow.*` rule
/// violation against one subject. Detail is a fixed bounded token;
/// the value itself never appears anywhere.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finding {
    pub(crate) rule_id: String,
    pub(crate) severity: Severity,
    pub(crate) subject: String,
    pub(crate) detail: String,
}

impl Finding {
    /// The registered rule id.
    pub fn rule_id(&self) -> &str {
        &self.rule_id
    }

    /// The finding severity.
    pub const fn severity(&self) -> Severity {
        self.severity
    }

    /// The subject reference (a bounded path string).
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// The fixed detail token.
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// The canonical wire form.
    pub(crate) fn to_wire(&self) -> Json {
        serde_json::json!({
            "ruleId": self.rule_id,
            "severity": self.severity.as_str(),
            "subject": self.subject,
            "detail": self.detail,
        })
    }

    /// Parse the wire form (row-level validation).
    pub(crate) fn from_wire(json: &Json) -> Result<Self, VocabularyError> {
        let object = json.as_object().ok_or(VocabularyError::Shape)?;
        let rule_id = object
            .get("ruleId")
            .and_then(Json::as_str)
            .ok_or(VocabularyError::Shape)?;
        if !rule_id.starts_with("classification.") && !rule_id.starts_with("dataflow.") {
            return Err(VocabularyError::Shape);
        }
        if rule_id.len() > 64 {
            return Err(VocabularyError::Length);
        }
        let severity = Severity::parse(
            object
                .get("severity")
                .and_then(Json::as_str)
                .ok_or(VocabularyError::Shape)?,
        )
        .ok_or(VocabularyError::Shape)?;
        let subject = object
            .get("subject")
            .and_then(Json::as_str)
            .ok_or(VocabularyError::Shape)?;
        if subject.is_empty() || subject.len() > 324 {
            return Err(VocabularyError::Length);
        }
        let detail = object
            .get("detail")
            .and_then(Json::as_str)
            .ok_or(VocabularyError::Shape)?;
        if detail.is_empty()
            || detail.len() > 64
            || !detail.starts_with(|byte: char| byte.is_ascii_lowercase())
        {
            return Err(VocabularyError::Shape);
        }
        Ok(Self {
            rule_id: rule_id.to_owned(),
            severity,
            subject: subject.to_owned(),
            detail: detail.to_owned(),
        })
    }
}

/// The closed unknown-reason vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum UnknownReason {
    /// `dynamic-hop`.
    DynamicHop,
    /// `foreign-implementation`.
    ForeignImplementation,
    /// `observed-incomplete`.
    ObservedIncomplete,
    /// `unresolved-subject`.
    UnresolvedSubject,
    /// `partial-adapter-outcome`.
    PartialAdapterOutcome,
}

impl UnknownReason {
    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "dynamic-hop" => Self::DynamicHop,
            "foreign-implementation" => Self::ForeignImplementation,
            "observed-incomplete" => Self::ObservedIncomplete,
            "unresolved-subject" => Self::UnresolvedSubject,
            "partial-adapter-outcome" => Self::PartialAdapterOutcome,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DynamicHop => "dynamic-hop",
            Self::ForeignImplementation => "foreign-implementation",
            Self::ObservedIncomplete => "observed-incomplete",
            Self::UnresolvedSubject => "unresolved-subject",
            Self::PartialAdapterOutcome => "partial-adapter-outcome",
        }
    }
}

impl Serialize for UnknownReason {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// One first-class unknown flow. Unknown is never safe: every entry
/// blocks the gates on its path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnknownFlow {
    pub(crate) source: SubjectPath,
    pub(crate) reason: UnknownReason,
}

impl UnknownFlow {
    /// The unknown-flow source subject.
    pub fn source(&self) -> &SubjectPath {
        &self.source
    }

    /// The closed reason.
    pub const fn reason(&self) -> UnknownReason {
        self.reason
    }

    /// The canonical wire form.
    pub(crate) fn to_wire(&self) -> Json {
        serde_json::json!({
            "source": self.source.as_str(),
            "reason": self.reason.as_str(),
        })
    }

    /// Parse the wire form (row-level validation).
    pub(crate) fn from_wire(json: &Json) -> Result<Self, VocabularyError> {
        let object = json.as_object().ok_or(VocabularyError::Shape)?;
        let source = SubjectPath::parse(
            object
                .get("source")
                .and_then(Json::as_str)
                .ok_or(VocabularyError::Shape)?,
        )
        .map_err(|_| VocabularyError::Shape)?;
        let reason = UnknownReason::parse(
            object
                .get("reason")
                .and_then(Json::as_str)
                .ok_or(VocabularyError::Shape)?,
        )
        .ok_or(VocabularyError::Shape)?;
        Ok(Self { source, reason })
    }
}

/// The bounded report text type, re-exported from classification.
pub type BoundedText = super::super::classification::types::BoundedText;
/// The bounded question id, re-exported from classification.
pub type QuestionId = super::super::classification::types::QuestionId;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabularies_parse_the_closed_sets() {
        for kind in SinkKind::ALL {
            assert_eq!(SinkKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(SinkKind::parse("database"), None);
        for confidence in [Confidence::High, Confidence::Low, Confidence::Unknown] {
            assert_eq!(Confidence::parse(confidence.as_str()), Some(confidence));
        }
        assert_eq!(Confidence::parse("certain"), None);
        for provenance in [
            Provenance::Canonical,
            Provenance::Observed,
            Provenance::Declared,
        ] {
            assert_eq!(Provenance::parse(provenance.as_str()), Some(provenance));
        }
        assert_eq!(Provenance::parse("guessed"), None);
        for reason in [
            TenantRelation::Same,
            TenantRelation::Crossing,
            TenantRelation::Unknown,
        ] {
            assert_eq!(TenantRelation::parse(reason.as_str()), Some(reason));
        }
        assert_eq!(TenantRelation::parse("shared"), None);
    }

    #[test]
    fn findings_reject_non_family_rules_and_values() {
        let finding = serde_json::json!({
            "ruleId": "dataflow.tenant-crossing",
            "severity": "error",
            "subject": "core.entity.user/email",
            "detail": "cross-tenant-event"
        });
        assert!(Finding::from_wire(&finding).is_ok());
        let foreign = serde_json::json!({
            "ruleId": "graph.input-invalid",
            "severity": "error",
            "subject": "x",
            "detail": "boom"
        });
        assert!(Finding::from_wire(&foreign).is_err());
        let long_detail = serde_json::json!({
            "ruleId": "dataflow.tenant-crossing",
            "severity": "error",
            "subject": "x.y/f",
            "detail": "d"
        });
        assert!(Finding::from_wire(&long_detail).is_ok());
    }

    #[test]
    fn flows_round_trip_through_wire() {
        let flow = Flow {
            id: "flow-1".to_owned(),
            source: SubjectPath::parse("core.entity.user/email").expect("valid"),
            path: vec!["effect:core.command.update_user".to_owned()],
            sink: "core.command.update_user".to_owned(),
            sink_kind: SinkKind::StorageWrite,
            provenance: Provenance::Canonical,
            confidence: Confidence::High,
            tenant_relation: TenantRelation::Same,
            classification: DataKind::Personal,
            gate: Some(Gate {
                required: false,
                satisfied: true,
                reason: GateReason::NotRequired,
            }),
        };
        let wire = flow.to_wire();
        let parsed = Flow::from_wire(&wire).expect("round trips");
        assert_eq!(parsed, flow);
    }
}
