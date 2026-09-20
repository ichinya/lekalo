//! The typed data-classification attachment and its canonical bytes
//! (issue #87).
//!
//! [`Attachment::parse`] is the single entry from exact JSON bytes to
//! the typed attachment. It fails closed before any semantic
//! processing: unknown or missing members, wrong identities, malformed
//! identifiers, digests, timestamps, bounds, closed-vocabulary
//! violations, and duplicate or contradicting declarations each
//! return one typed registered diagnostic and no partial value.
//! Collections normalize to canonical order (classification entries
//! and grants sort by subject bytes; open questions by id) so the
//! canonical bytes of equivalent documents compare equal.

use serde::Serialize;
use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::Sha256Digest;
use crate::scenario::id::SemanticId;

use super::diagnostic;
use super::types::{
    BoundedText, Condition, ContractRef, DataKind, IsoTimestamp, Label, Profile,
    QuestionId, ReviewRef, RetentionClass, SubjectPath,
};
use super::version;

/// The closed top-level member set of the attachment.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "attachmentRevision",
    "projectId",
    "modelRef",
    "irRef",
    "defaults",
    "classifications",
    "declassifications",
    "openQuestions",
];

/// The closed defaults member set.
const DEFAULTS_KEYS: &[&str] = &[
    "profile",
    "unclassifiedFields",
    "unclassifiedPayloads",
];

/// The closed classification-entry member set.
const CLASSIFICATION_KEYS: &[&str] = &[
    "subject",
    "kind",
    "tenantScoped",
    "retentionClass",
    "labels",
];

/// The closed declassification-grant member set.
const GRANT_KEYS: &[&str] = &[
    "id",
    "subject",
    "fromKind",
    "toKind",
    "approvedBy",
    "justification",
    "conditions",
    "expiresAt",
];

/// The closed open-question member set.
const OPEN_QUESTION_KEYS: &[&str] = &["id", "question", "subject"];

/// One declared classification: one subject, its kind, and bounded
/// optional metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Classification {
    subject: SubjectPath,
    kind: DataKind,
    tenant_scoped: bool,
    retention_class: Option<RetentionClass>,
    labels: Vec<Label>,
}

impl Classification {
    /// The classified subject.
    pub fn subject(&self) -> &SubjectPath {
        &self.subject
    }

    /// The declared kind.
    pub const fn kind(&self) -> DataKind {
        self.kind
    }

    /// Whether the subject is explicitly tenant-scoped.
    pub const fn tenant_scoped(&self) -> bool {
        self.tenant_scoped
    }

    /// The optional retention class.
    pub fn retention_class(&self) -> Option<&RetentionClass> {
        self.retention_class.as_ref()
    }

    /// The bounded labels, canonically ordered.
    pub fn labels(&self) -> &[Label] {
        &self.labels
    }
}

/// One subject-bound declassification grant: a reviewed, justified,
/// optionally conditioned and expiring permission to lower one
/// subject's kind. Never implicit; never available to `credential`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Declassification {
    id: ContractRef,
    subject: SubjectPath,
    from_kind: DataKind,
    to_kind: DataKind,
    approved_by: ReviewRef,
    justification: BoundedText,
    conditions: Vec<Condition>,
    expires_at: Option<IsoTimestamp>,
}

impl Declassification {
    /// The exact grant identity.
    pub fn id(&self) -> &ContractRef {
        &self.id
    }

    /// The granted subject.
    pub fn subject(&self) -> &SubjectPath {
        &self.subject
    }

    /// The kind being lowered.
    pub const fn from_kind(&self) -> DataKind {
        self.from_kind
    }

    /// The kind being reached.
    pub const fn to_kind(&self) -> DataKind {
        self.to_kind
    }

    /// The opaque review reference.
    pub fn approved_by(&self) -> &ReviewRef {
        &self.approved_by
    }

    /// The bounded justification.
    pub fn justification(&self) -> &BoundedText {
        &self.justification
    }

    /// The closed conditions, canonically ordered, deduplicated.
    pub fn conditions(&self) -> &[Condition] {
        &self.conditions
    }

    /// The optional expiry timestamp.
    pub fn expires_at(&self) -> Option<&IsoTimestamp> {
        self.expires_at.as_ref()
    }
}

/// One registered open question.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenQuestion {
    id: QuestionId,
    question: BoundedText,
    subject: Option<SubjectPath>,
}

impl OpenQuestion {
    /// The question id.
    pub fn id(&self) -> &QuestionId {
        &self.id
    }

    /// The bounded question text.
    pub fn question(&self) -> &BoundedText {
        &self.question
    }

    /// The optional related subject.
    pub fn subject(&self) -> Option<&SubjectPath> {
        self.subject.as_ref()
    }
}

/// The per-profile secure defaults.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Defaults {
    profile: Profile,
    unclassified_fields: DataKind,
    unclassified_payloads: DataKind,
}

impl Defaults {
    /// The declared profile.
    pub const fn profile(&self) -> Profile {
        self.profile
    }

    /// The default kind for unclassified field subjects.
    pub const fn unclassified_fields(&self) -> DataKind {
        self.unclassified_fields
    }

    /// The default kind for unclassified payload subjects.
    pub const fn unclassified_payloads(&self) -> DataKind {
        self.unclassified_payloads
    }
}

/// The typed data-classification attachment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attachment {
    attachment_revision: String,
    project_id: SemanticId,
    model_ref: (String, Sha256Digest),
    ir_ref: (String, Sha256Digest),
    defaults: Defaults,
    classifications: Vec<Classification>,
    declassifications: Vec<Declassification>,
    open_questions: Vec<OpenQuestion>,
}

impl Attachment {
    /// Assemble from validated parts (wire internal); collections are
    /// stored in the caller's canonical order.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        attachment_revision: String,
        project_id: SemanticId,
        model_ref: (String, Sha256Digest),
        ir_ref: (String, Sha256Digest),
        defaults: Defaults,
        classifications: Vec<Classification>,
        declassifications: Vec<Declassification>,
        open_questions: Vec<OpenQuestion>,
    ) -> Self {
        Self {
            attachment_revision,
            project_id,
            model_ref,
            ir_ref,
            defaults,
            classifications,
            declassifications,
            open_questions,
        }
    }

    /// Normalize one wire document into a validated attachment, or
    /// return the typed rejection set.
    pub fn from_value(json: &Json) -> Result<Self, DiagnosticSet> {
        from_value(json)
    }

    /// Parse one attachment document (exact UTF-8 JSON bytes) or
    /// return the terminal rejection set.
    pub fn parse(bytes: &[u8]) -> Result<Self, DiagnosticSet> {
        if bytes.len() > version::MAX_DOC_BYTES {
            return Err(diagnostic::document_invalid(
                "document-bytes",
                None,
            ));
        }
        let text =
            std::str::from_utf8(bytes).map_err(|_| diagnostic::document_invalid("invalid-encoding", None))?;
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

    /// The per-profile secure defaults.
    pub const fn defaults(&self) -> &Defaults {
        &self.defaults
    }

    /// Every classification entry, canonically ordered by subject.
    pub fn classifications(&self) -> &[Classification] {
        &self.classifications
    }

    /// Every declassification grant, canonically ordered by subject
    /// then grant id.
    pub fn declassifications(&self) -> &[Declassification] {
        &self.declassifications
    }

    /// Every registered open question, canonically ordered by id.
    pub fn open_questions(&self) -> &[OpenQuestion] {
        &self.open_questions
    }
}

/// Normalize one wire document into a validated attachment, or return
/// the typed rejection set with no partial attachment. Pure: no
/// model, filesystem, cache, report, network, process, or target
/// access of any kind.
fn from_value(json: &Json) -> Result<Attachment, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("top-level-shape", None))?;
    check_members(object, TOP_LEVEL_KEYS)?;
    if object.get("schemaVersion").and_then(Json::as_str) != Some(version::SCHEMA_VERSION) {
        return Err(diagnostic::document_invalid("schema-version", None));
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::IDENTITY) {
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
    let defaults = defaults(
        object
            .get("defaults")
            .ok_or_else(|| diagnostic::document_invalid("defaults", None))?,
    )?;
    let classifications = classifications(
        object
            .get("classifications")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::document_invalid("classification-list", None))?,
    )?;
    let declassifications = declassifications(
        object
            .get("declassifications")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::document_invalid("declassification-list", None))?,
    )?;
    let open_questions = open_questions(
        object
            .get("openQuestions")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::document_invalid("open-question-list", None))?,
    )?;
    Ok(Attachment::assemble(
        attachment_revision,
        project_id,
        model_ref,
        ir_ref,
        defaults,
        classifications,
        declassifications,
        open_questions,
    ))
}

/// Reject any member outside the closed set.
pub(crate) fn check_members(
    object: &serde_json::Map<String, Json>,
    keys: &[&str],
) -> Result<(), DiagnosticSet> {
    for key in object.keys() {
        if !keys.contains(&key.as_str()) {
            return Err(diagnostic::document_invalid("unknown-field", None));
        }
    }
    Ok(())
}

/// One canonical SemVer spelling without build metadata.
pub(crate) fn semver(text: &str, detail: &'static str) -> Result<String, DiagnosticSet> {
    let parsed =
        semver::Version::parse(text).map_err(|_| diagnostic::document_invalid(detail, None))?;
    if parsed.build != semver::BuildMetadata::EMPTY || parsed.to_string() != text {
        return Err(diagnostic::document_invalid(detail, None));
    }
    Ok(text.to_owned())
}

/// The bound Model pin: exact accepted version plus payload digest.
pub(crate) fn model_pin(json: &Json) -> Result<(String, Sha256Digest), DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("model-ref", None))?;
    check_members(object, &["modelVersion", "digest"])?;
    let model_version = object
        .get("modelVersion")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::document_invalid("model-version", None))?;
    if model_version != version::MODEL_VERSION {
        return Err(diagnostic::document_invalid("model-version", None));
    }
    let digest = Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("model-digest", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("model-digest", None))?;
    Ok((model_version.to_owned(), digest))
}

/// The bound IR pin: exact accepted version plus canonical IR digest.
pub(crate) fn ir_pin(json: &Json) -> Result<(String, Sha256Digest), DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("ir-ref", None))?;
    check_members(object, &["irVersion", "digest"])?;
    let ir_version = object
        .get("irVersion")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::document_invalid("ir-version", None))?;
    if ir_version != "0.2.16" {
        return Err(diagnostic::document_invalid("ir-version", None));
    }
    let digest = Sha256Digest::parse(
        object
            .get("digest")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("ir-digest", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("ir-digest", None))?;
    Ok((ir_version.to_owned(), digest))
}

/// The per-profile secure defaults.
fn defaults(json: &Json) -> Result<Defaults, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("defaults", None))?;
    check_members(object, DEFAULTS_KEYS)?;
    let profile = Profile::parse(
        object
            .get("profile")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("profile", None))?,
    )
    .ok_or_else(|| diagnostic::document_invalid("profile", None))?;
    let unclassified_fields = DataKind::parse(
        object
            .get("unclassifiedFields")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("unclassified-fields", None))?,
    )
    .ok_or_else(|| diagnostic::document_invalid("unclassified-fields", None))?;
    let unclassified_payloads = DataKind::parse(
        object
            .get("unclassifiedPayloads")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("unclassified-payloads", None))?,
    )
    .ok_or_else(|| diagnostic::document_invalid("unclassified-payloads", None))?;
    Ok(Defaults {
        profile,
        unclassified_fields,
        unclassified_payloads,
    })
}

/// Parse every classification entry; sort canonically by subject and
/// reject exact duplicates.
fn classifications(json: &[Json]) -> Result<Vec<Classification>, DiagnosticSet> {
    if json.len() > version::MAX_CLASSIFICATIONS {
        return Err(diagnostic::document_invalid(
            "classification-list",
            None,
        ));
    }
    let mut parsed = Vec::with_capacity(json.len());
    for value in json {
        let object = value
            .as_object()
            .ok_or_else(|| diagnostic::document_invalid("classification-shape", None))?;
        check_members(object, CLASSIFICATION_KEYS)?;
        let subject = SubjectPath::parse(
            object
                .get("subject")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("subject", None))?,
        )
        .map_err(|_| diagnostic::document_invalid("subject", None))?;
        let kind = DataKind::parse(
            object
                .get("kind")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("kind", None))?,
        )
        .ok_or_else(|| diagnostic::document_invalid("kind", None))?;
        let tenant_scoped = match object.get("tenantScoped") {
            None => false,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| diagnostic::document_invalid("tenant-scoped", None))?,
        };
        let retention_class = match object.get("retentionClass") {
            None => None,
            Some(value) => Some(
                RetentionClass::parse(
                    value
                        .as_str()
                        .ok_or_else(|| diagnostic::document_invalid("retention-class", None))?,
                )
                .map_err(|_| diagnostic::document_invalid("retention-class", None))?,
            ),
        };
        let labels = match object.get("labels") {
            None => Vec::new(),
            Some(value) => {
                let items = value
                    .as_array()
                    .ok_or_else(|| diagnostic::document_invalid("labels", None))?;
                if items.len() > version::MAX_LABELS {
                    return Err(diagnostic::document_invalid("labels", None));
                }
                let mut parsed_labels = Vec::with_capacity(items.len());
                for item in items {
                    parsed_labels.push(
                        Label::parse(
                            item.as_str()
                                .ok_or_else(|| diagnostic::document_invalid("labels", None))?,
                        )
                        .map_err(|_| diagnostic::document_invalid("labels", None))?,
                    );
                }
                parsed_labels.sort();
                parsed_labels.dedup();
                parsed_labels
            }
        };
        parsed.push(Classification {
            subject,
            kind,
            tenant_scoped,
            retention_class,
            labels,
        });
    }
    parsed.sort_by(|left, right| left.subject().as_str().cmp(right.subject().as_str()));
    for pair in parsed.windows(2) {
        if pair[0].subject().as_str() == pair[1].subject().as_str() {
            return Err(diagnostic::document_invalid(
                "duplicate-classification",
                Some(pair[0].subject().as_str()),
            ));
        }
    }
    Ok(parsed)
}

/// Parse every declassification grant; sort canonically by subject
/// then grant id and reject exact subject duplicates.
fn declassifications(json: &[Json]) -> Result<Vec<Declassification>, DiagnosticSet> {
    if json.len() > version::MAX_DECLASSIFICATIONS {
        return Err(diagnostic::document_invalid(
            "declassification-list",
            None,
        ));
    }
    let mut parsed = Vec::with_capacity(json.len());
    for value in json {
        let object = value
            .as_object()
            .ok_or_else(|| diagnostic::document_invalid("grant-shape", None))?;
        check_members(object, GRANT_KEYS)?;
        let id = ContractRef::parse(
            object
                .get("id")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("grant-id", None))?,
        )
        .map_err(|_| diagnostic::document_invalid("grant-id", None))?;
        let subject = SubjectPath::parse(
            object
                .get("subject")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("subject", None))?,
        )
        .map_err(|_| diagnostic::document_invalid("subject", None))?;
        let from_kind = DataKind::parse(
            object
                .get("fromKind")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("from-kind", None))?,
        )
        .ok_or_else(|| diagnostic::document_invalid("from-kind", None))?;
        let to_kind = DataKind::parse(
            object
                .get("toKind")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("to-kind", None))?,
        )
        .ok_or_else(|| diagnostic::document_invalid("to-kind", None))?;
        let approved_by = ReviewRef::parse(
            object
                .get("approvedBy")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("approved-by", None))?,
        )
        .map_err(|_| diagnostic::document_invalid("approved-by", None))?;
        let justification = BoundedText::parse(
            object
                .get("justification")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid("justification", None))?,
        )
        .map_err(|_| diagnostic::document_invalid("justification", None))?;
        let conditions = match object.get("conditions") {
            None => Vec::new(),
            Some(value) => {
                let items = value
                    .as_array()
                    .ok_or_else(|| diagnostic::document_invalid("conditions", None))?;
                if items.len() > version::MAX_CONDITIONS {
                    return Err(diagnostic::document_invalid("conditions", None));
                }
                let mut parsed_conditions = Vec::with_capacity(items.len());
                for item in items {
                    parsed_conditions.push(
                        Condition::parse(
                            item.as_str()
                                .ok_or_else(|| diagnostic::document_invalid("conditions", None))?,
                        )
                        .ok_or_else(|| diagnostic::document_invalid("conditions", None))?,
                    );
                }
                parsed_conditions.sort();
                parsed_conditions.dedup();
                parsed_conditions
            }
        };
        let expires_at = match object.get("expiresAt") {
            None => None,
            Some(value) => Some(
                IsoTimestamp::parse(
                    value
                        .as_str()
                        .ok_or_else(|| diagnostic::document_invalid("expires-at", None))?,
                )
                .map_err(|_| diagnostic::document_invalid("expires-at", None))?,
            ),
        };
        parsed.push(Declassification {
            id,
            subject,
            from_kind,
            to_kind,
            approved_by,
            justification,
            conditions,
            expires_at,
        });
    }
    parsed.sort_by(|left, right| {
        left.subject()
            .as_str()
            .cmp(right.subject().as_str())
            .then_with(|| left.id().as_str().cmp(right.id().as_str()))
    });
    for pair in parsed.windows(2) {
        if pair[0].subject().as_str() == pair[1].subject().as_str()
            && pair[0].to_kind() != pair[1].to_kind()
        {
            return Err(diagnostic::document_invalid(
                "contradicting-grant",
                Some(pair[0].subject().as_str()),
            ));
        }
    }
    Ok(parsed)
}

/// Parse every open question; sort canonically by id.
fn open_questions(json: &[Json]) -> Result<Vec<OpenQuestion>, DiagnosticSet> {
    if json.len() > version::MAX_OPEN_QUESTIONS {
        return Err(diagnostic::document_invalid("open-question-list", None));
    }
    let mut parsed = Vec::with_capacity(json.len());
    for value in json {
        let object = value
            .as_object()
            .ok_or_else(|| diagnostic::document_invalid("open-question-shape", None))?;
        check_members(object, OPEN_QUESTION_KEYS)?;
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
        let subject = match object.get("subject") {
            None => None,
            Some(value) => Some(
                SubjectPath::parse(
                    value
                        .as_str()
                        .ok_or_else(|| diagnostic::document_invalid("subject", None))?,
                )
                .map_err(|_| diagnostic::document_invalid("subject", None))?,
            ),
        };
        parsed.push(OpenQuestion {
            id,
            question,
            subject,
        });
    }
    parsed.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
    for pair in parsed.windows(2) {
        if pair[0].id().as_str() == pair[1].id().as_str() {
            return Err(diagnostic::document_invalid(
                "duplicate-question",
                None,
            ));
        }
    }
    Ok(parsed)
}

/// The canonical wire view of the attachment: borrowed strings in the
/// closed member set, serialized with byte-sorted object keys by the
/// canonical writer.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AttachmentWire<'a> {
    schema_version: &'static str,
    identity: &'static str,
    attachment_revision: &'a str,
    project_id: &'a str,
    model_ref: ModelRefWire<'a>,
    ir_ref: IrRefWire<'a>,
    defaults: DefaultsWire<'a>,
    classifications: Vec<ClassificationWire<'a>>,
    declassifications: Vec<GrantWire<'a>>,
    open_questions: Vec<OpenQuestionWire<'a>>,
}

/// The canonical wire view of the Model pin.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelRefWire<'a> {
    pub(crate) model_version: &'a str,
    pub(crate) digest: &'a str,
}

/// The canonical wire view of the IR pin.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IrRefWire<'a> {
    pub(crate) ir_version: &'a str,
    pub(crate) digest: &'a str,
}

/// The canonical wire view of the defaults.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DefaultsWire<'a> {
    profile: &'a str,
    unclassified_fields: &'a str,
    unclassified_payloads: &'a str,
}

/// The canonical wire view of one classification entry.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClassificationWire<'a> {
    subject: &'a str,
    kind: &'a str,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    tenant_scoped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    retention_class: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    labels: Vec<&'a str>,
}

/// The canonical wire view of one declassification grant.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GrantWire<'a> {
    id: &'a str,
    subject: &'a str,
    from_kind: &'a str,
    to_kind: &'a str,
    approved_by: &'a str,
    justification: &'a str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    conditions: Vec<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<&'a str>,
}

/// The canonical wire view of one open question.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenQuestionWire<'a> {
    id: &'a str,
    question: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<&'a str>,
}

impl Attachment {
    /// The canonical borrowed wire view (canonical internal).
    pub(crate) fn wire(&self) -> AttachmentWire<'_> {
        AttachmentWire {
            schema_version: version::SCHEMA_VERSION,
            identity: version::IDENTITY,
            attachment_revision: &self.attachment_revision,
            project_id: self.project_id.as_str(),
            model_ref: ModelRefWire {
                model_version: &self.model_ref.0,
                digest: self.model_ref.1.as_str(),
            },
            ir_ref: IrRefWire {
                ir_version: &self.ir_ref.0,
                digest: self.ir_ref.1.as_str(),
            },
            defaults: DefaultsWire {
                profile: self.defaults.profile().as_str(),
                unclassified_fields: self.defaults.unclassified_fields().as_str(),
                unclassified_payloads: self.defaults.unclassified_payloads().as_str(),
            },
            classifications: self
                .classifications
                .iter()
                .map(|entry| ClassificationWire {
                    subject: entry.subject().as_str(),
                    kind: entry.kind().as_str(),
                    tenant_scoped: entry.tenant_scoped(),
                    retention_class: entry.retention_class().map(RetentionClass::as_str),
                    labels: entry.labels().iter().map(|label| label.as_str()).collect(),
                })
                .collect(),
            declassifications: self
                .declassifications
                .iter()
                .map(|grant| GrantWire {
                    id: grant.id().as_str(),
                    subject: grant.subject().as_str(),
                    from_kind: grant.from_kind().as_str(),
                    to_kind: grant.to_kind().as_str(),
                    approved_by: grant.approved_by().as_str(),
                    justification: grant.justification().as_str(),
                    conditions: grant
                        .conditions()
                        .iter()
                        .map(|condition| condition.as_str())
                        .collect(),
                    expires_at: grant.expires_at().map(IsoTimestamp::as_str),
                })
                .collect(),
            open_questions: self
                .open_questions
                .iter()
                .map(|question| OpenQuestionWire {
                    id: question.id().as_str(),
                    question: question.question().as_str(),
                    subject: question.subject().map(SubjectPath::as_str),
                })
                .collect(),
        }
    }
}
