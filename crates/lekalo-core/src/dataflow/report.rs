//! The typed data-flow report and its canonical bytes (issue #87).
//!
//! The report is the derived read-only output of the data-flow
//! analysis: flows, findings, unknowns, and the aggregated gate
//! verdict, pinned to the exact canonical digests of the
//! classification and policy inputs. Unknown and low-confidence are
//! first-class states; a report never silently omits an unresolved
//! flow. The report is metadata-only: findings reference subjects by
//! path and carry fixed bounded detail tokens — never values.

use serde::Serialize;
use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::Sha256Digest;
use crate::scenario::id::SemanticId;

use super::diagnostic;
use super::types::{
    BoundedText, Finding, Flow, QuestionId, UnknownFlow,
};
use super::version;

/// The closed top-level member set of the report.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "reportRevision",
    "projectId",
    "modelRef",
    "irRef",
    "classificationRef",
    "policyRef",
    "generatedBy",
    "inputsComplete",
    "verdict",
    "flows",
    "findings",
    "unknowns",
    "openQuestions",
];

/// One registered report open question.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportQuestion {
    id: QuestionId,
    question: BoundedText,
}

impl ReportQuestion {
    /// The question id.
    pub fn id(&self) -> &QuestionId {
        &self.id
    }

    /// The bounded question text.
    pub fn question(&self) -> &BoundedText {
        &self.question
    }
}

/// The typed data-flow report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    report_revision: String,
    project_id: SemanticId,
    model_ref: (String, Sha256Digest),
    ir_ref: (String, Sha256Digest),
    classification_ref: Sha256Digest,
    policy_ref: Sha256Digest,
    generated_by: String,
    inputs_complete: bool,
    verdict: Verdict,
    flows: Vec<Flow>,
    findings: Vec<Finding>,
    unknowns: Vec<UnknownFlow>,
    open_questions: Vec<ReportQuestion>,
}

/// The aggregated gate verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// Every gate satisfied and no error-severity finding.
    Pass,
    /// At least one gate unsatisfied or one error-severity finding.
    Denied,
}

impl Verdict {
    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Denied => "denied",
        }
    }
}

impl Serialize for Verdict {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl Report {
    /// Assemble from validated parts (builder internal); collections
    /// are stored in the caller's canonical order.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn assemble(
        report_revision: String,
        project_id: SemanticId,
        model_ref: (String, Sha256Digest),
        ir_ref: (String, Sha256Digest),
        classification_ref: Sha256Digest,
        policy_ref: Sha256Digest,
        generated_by: String,
        inputs_complete: bool,
        flows: Vec<Flow>,
        findings: Vec<Finding>,
        unknowns: Vec<UnknownFlow>,
        open_questions: Vec<ReportQuestion>,
    ) -> Self {
        let verdict = verdict_of(&findings, inputs_complete);
        Self {
            report_revision,
            project_id,
            model_ref,
            ir_ref,
            classification_ref,
            policy_ref,
            generated_by,
            inputs_complete,
            verdict,
            flows,
            findings,
            unknowns,
            open_questions,
        }
    }

    /// The exact report revision.
    pub fn report_revision(&self) -> &str {
        &self.report_revision
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

    /// The exact canonical digest of the classification input.
    pub fn classification_ref(&self) -> &Sha256Digest {
        &self.classification_ref
    }

    /// The exact canonical digest of the policy input.
    pub fn policy_ref(&self) -> &Sha256Digest {
        &self.policy_ref
    }

    /// The engine identity that derived this report.
    pub fn generated_by(&self) -> &str {
        &self.generated_by
    }

    /// Whether every input section was complete; false blocks gate
    /// satisfaction project-wide.
    pub const fn inputs_complete(&self) -> bool {
        self.inputs_complete
    }

    /// The aggregated verdict.
    pub const fn verdict(&self) -> Verdict {
        self.verdict
    }

    /// Every flow, canonically ordered.
    pub fn flows(&self) -> &[Flow] {
        &self.flows
    }

    /// Every finding, canonically ordered.
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// Every unknown flow, canonically ordered.
    pub fn unknowns(&self) -> &[UnknownFlow] {
        &self.unknowns
    }

    /// Every open question, canonically ordered.
    pub fn open_questions(&self) -> &[ReportQuestion] {
        &self.open_questions
    }
}

/// Derive the aggregated verdict: denied when any error-severity
/// finding exists or inputs were incomplete (unknown is never safe).
fn verdict_of(findings: &[Finding], inputs_complete: bool) -> Verdict {
    if !inputs_complete || findings.iter().any(|finding| finding.severity().is_error()) {
        Verdict::Denied
    } else {
        Verdict::Pass
    }
}

/// Parse one report document (exact UTF-8 JSON bytes) or return the
/// terminal rejection set. The report is a derived artifact: parsing
/// validates shape and identity so exported reports can be re-read,
/// but a parsed report is never re-input to analysis.
pub fn report_from_value(json: &Json) -> Result<Report, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid("top-level-shape", None))?;
    for key in object.keys() {
        if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(diagnostic::document_invalid("unknown-field", None));
        }
    }
    if object.get("schemaVersion").and_then(Json::as_str) != Some(version::SCHEMA_VERSION) {
        return Err(diagnostic::document_invalid("schema-version", None));
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::IDENTITY) {
        return Err(diagnostic::document_invalid("contract-identity", None));
    }
    // Full wire round-trip validation of every row.
    let flows = json
        .get("flows")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::document_invalid("flow-list", None))?;
    if flows.len() > version::MAX_FLOWS {
        return Err(diagnostic::document_invalid("flow-list", None));
    }
    for flow in flows {
        Flow::from_wire(flow).map_err(|_| diagnostic::document_invalid("flow-row", None))?;
    }
    let findings = json
        .get("findings")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::document_invalid("finding-list", None))?;
    if findings.len() > version::MAX_FINDINGS {
        return Err(diagnostic::document_invalid("finding-list", None));
    }
    for finding in findings {
        Finding::from_wire(finding)
            .map_err(|_| diagnostic::document_invalid("finding-row", None))?;
    }
    let unknowns = json
        .get("unknowns")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::document_invalid("unknown-list", None))?;
    if unknowns.len() > version::MAX_UNKNOWNS {
        return Err(diagnostic::document_invalid("unknown-list", None));
    }
    for unknown in unknowns {
        UnknownFlow::from_wire(unknown)
            .map_err(|_| diagnostic::document_invalid("unknown-row", None))?;
    }
    // The typed report is assembled only by the builder; re-reading an
    // exported report yields its validated rows for diffing.
    report_from_parts(json, flows, findings, unknowns)
}

/// Assemble the typed report from validated wire rows.
#[allow(clippy::too_many_arguments)]
fn report_from_parts(
    json: &Json,
    flows: &[Json],
    findings: &[Json],
    unknowns: &[Json],
) -> Result<Report, DiagnosticSet> {
    let object = json.as_object().expect("checked object");
    let report_revision = wire_semver(
        object
            .get("reportRevision")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("report-revision", None))?,
    )?;
    let project_id = SemanticId::parse_root(
        object
            .get("projectId")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("project-id", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("project-id", None))?;
    let (model_ref, ir_ref) = custody_pins(object)?;
    let classification_ref = Sha256Digest::parse(
        object
            .get("classificationRef")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("classification-ref", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("classification-ref", None))?;
    let policy_ref = Sha256Digest::parse(
        object
            .get("policyRef")
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid("policy-ref", None))?,
    )
    .map_err(|_| diagnostic::document_invalid("policy-ref", None))?;
    let generated_by = object
        .get("generatedBy")
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::document_invalid("generated-by", None))?;
    if generated_by.is_empty() || generated_by.len() > 64 {
        return Err(diagnostic::document_invalid("generated-by", None));
    }
    let inputs_complete = object
        .get("inputsComplete")
        .and_then(Json::as_bool)
        .ok_or_else(|| diagnostic::document_invalid("inputs-complete", None))?;
    let flows = flows
        .iter()
        .map(|flow| Flow::from_wire(flow).map_err(|_| diagnostic::document_invalid("flow-row", None)))
        .collect::<Result<Vec<_>, _>>()?;
    let findings = findings
        .iter()
        .map(|finding| {
            Finding::from_wire(finding).map_err(|_| diagnostic::document_invalid("finding-row", None))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let unknowns = unknowns
        .iter()
        .map(|unknown| {
            UnknownFlow::from_wire(unknown)
                .map_err(|_| diagnostic::document_invalid("unknown-row", None))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let open_questions = match json.get("openQuestions").and_then(Json::as_array) {
        None => Vec::new(),
        Some(items) => {
            if items.len() > version::MAX_OPEN_QUESTIONS {
                return Err(diagnostic::document_invalid("open-question-list", None));
            }
            let mut parsed = Vec::with_capacity(items.len());
            for item in items {
                let question_object = item
                    .as_object()
                    .ok_or_else(|| diagnostic::document_invalid("open-question-shape", None))?;
                let id = QuestionId::parse(
                    question_object
                        .get("id")
                        .and_then(Json::as_str)
                        .ok_or_else(|| diagnostic::document_invalid("question-id", None))?,
                )
                .map_err(|_| diagnostic::document_invalid("question-id", None))?;
                let question = BoundedText::parse(
                    question_object
                        .get("question")
                        .and_then(Json::as_str)
                        .ok_or_else(|| diagnostic::document_invalid("question", None))?,
                )
                .map_err(|_| diagnostic::document_invalid("question", None))?;
                parsed.push(ReportQuestion { id, question });
            }
            parsed
        }
    };
    let verdict = match object.get("verdict").and_then(Json::as_str) {
        Some("pass") => Verdict::Pass,
        Some("denied") => Verdict::Denied,
        _ => return Err(diagnostic::document_invalid("verdict", None)),
    };
    Ok(Report {
        report_revision,
        project_id,
        model_ref,
        ir_ref,
        classification_ref,
        policy_ref,
        generated_by: generated_by.to_owned(),
        inputs_complete,
        verdict,
        flows,
        findings,
        unknowns,
        open_questions,
    })
}

/// The custody pins of the report.
type CustodyPin = (String, Sha256Digest);

fn custody_pins(
    object: &serde_json::Map<String, Json>,
) -> Result<(CustodyPin, CustodyPin), DiagnosticSet> {
    let pin = |key: &str, version_key: &str, detail: &'static str| -> Result<
        (String, Sha256Digest),
        DiagnosticSet,
    > {
        let pin_object = object
            .get(key)
            .ok_or_else(|| diagnostic::document_invalid(detail, None))?
            .as_object()
            .ok_or_else(|| diagnostic::document_invalid(detail, None))?;
        let version_text = pin_object
            .get(version_key)
            .and_then(Json::as_str)
            .ok_or_else(|| diagnostic::document_invalid(detail, None))?;
        let digest = Sha256Digest::parse(
            pin_object
                .get("digest")
                .and_then(Json::as_str)
                .ok_or_else(|| diagnostic::document_invalid(detail, None))?,
        )
        .map_err(|_| diagnostic::document_invalid(detail, None))?;
        Ok((version_text.to_owned(), digest))
    };
    let model_ref = pin("modelRef", "modelVersion", "model-ref")?;
    let ir_ref = pin("irRef", "irVersion", "ir-ref")?;
    Ok((model_ref, ir_ref))
}

/// The canonical wire view of the report: borrowed strings in the
/// closed member set, serialized with byte-sorted object keys by the
/// canonical writer.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportWire<'a> {
    schema_version: &'static str,
    identity: &'static str,
    report_revision: &'a str,
    project_id: &'a str,
    model_ref: ModelRefWire<'a>,
    ir_ref: IrRefWire<'a>,
    classification_ref: &'a str,
    policy_ref: &'a str,
    generated_by: &'a str,
    inputs_complete: bool,
    verdict: &'static str,
    flows: Vec<serde_json::Value>,
    findings: Vec<serde_json::Value>,
    unknowns: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    open_questions: Vec<ReportQuestionWire<'a>>,
}

/// The canonical wire view of the Model pin.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRefWire<'a> {
    pub model_version: &'a str,
    pub digest: &'a str,
}

/// The canonical wire view of the IR pin.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IrRefWire<'a> {
    pub ir_version: &'a str,
    pub digest: &'a str,
}

/// The canonical wire view of one open question.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportQuestionWire<'a> {
    id: &'a str,
    question: &'a str,
}

impl Report {
    /// The canonical borrowed wire view (canonical internal).
    pub(crate) fn wire(&self) -> ReportWire<'_> {
        ReportWire {
            schema_version: version::SCHEMA_VERSION,
            identity: version::IDENTITY,
            report_revision: &self.report_revision,
            project_id: self.project_id.as_str(),
            model_ref: ModelRefWire {
                model_version: &self.model_ref.0,
                digest: self.model_ref.1.as_str(),
            },
            ir_ref: IrRefWire {
                ir_version: &self.ir_ref.0,
                digest: self.ir_ref.1.as_str(),
            },
            classification_ref: self.classification_ref.as_str(),
            policy_ref: self.policy_ref.as_str(),
            generated_by: &self.generated_by,
            inputs_complete: self.inputs_complete,
            verdict: self.verdict.as_str(),
            flows: self.flows.iter().map(Flow::to_wire).collect(),
            findings: self.findings.iter().map(Finding::to_wire).collect(),
            unknowns: self.unknowns.iter().map(UnknownFlow::to_wire).collect(),
            open_questions: self
                .open_questions
                .iter()
                .map(|question| ReportQuestionWire {
                    id: question.id().as_str(),
                    question: question.question().as_str(),
                })
                .collect(),
        }
    }

    /// The canonical compact JSON bytes with byte-sorted keys.
    pub(crate) fn wire_bytes(&self) -> String {
        let dynamic = serde_json::to_value(self.wire()).expect("report wire serializes");
        serde_json::to_string(&dynamic).expect("canonical JSON bytes fit in memory")
    }
}

/// The canonical export of one report: compact JSON with byte-sorted
/// keys, canonical collections, and no trailing LF, or the
/// export-limit refusal.
pub fn report_canonical_bytes(report: &Report) -> Result<String, DiagnosticSet> {
    let bytes = report.wire_bytes();
    if bytes.len() > version::MAX_CANONICAL_BYTES {
        return Err(diagnostic::document_invalid("canonical-bytes", None));
    }
    Ok(bytes)
}

/// One canonical SemVer spelling without build metadata (report wire).
fn wire_semver(text: &str) -> Result<String, DiagnosticSet> {
    let parsed =
        semver::Version::parse(text).map_err(|_| diagnostic::document_invalid("report-revision", None))?;
    if parsed.build != semver::BuildMetadata::EMPTY || parsed.to_string() != text {
        return Err(diagnostic::document_invalid("report-revision", None));
    }
    Ok(text.to_owned())
}
