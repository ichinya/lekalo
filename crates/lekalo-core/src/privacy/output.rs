//! The typed export-decision output (issue #119).
//!
//! [`ExportDecisionOutput`] is the exact closed member set of the
//! frozen output schema: exactly one decision, at least one stable
//! reason code, the required transforms, the effective exact contract
//! references, the derived-artifact requirements, and whether the
//! evaluated source may transfer. `transform-required` never
//! authorizes transfer of the source.

use serde::Serialize;

use super::types::SchemaRef;
use super::types::{
    AuthorityRef as AuthorityRefWire, ClassificationContractRef, DecisionContractRef,
    EvidenceContractRef, PolicyRef, SubjectProfileRef,
};

/// The closed decision vocabulary: exactly one of `allow`, `deny`, or
/// `transform-required`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportDecision {
    /// `allow`.
    Allow,
    /// `deny`.
    Deny,
    /// `transform-required`.
    TransformRequired,
}

impl ExportDecision {
    /// Parse the exact wire spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "allow" => Self::Allow,
            "deny" => Self::Deny,
            "transform-required" => Self::TransformRequired,
            _ => return None,
        })
    }

    /// The exact wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::TransformRequired => "transform-required",
        }
    }
}

/// The exact effective contract references of every decision output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveRefs {
    decision_contract_ref: DecisionContractRef,
    authority_ref: AuthorityRefWire,
    policy_ref: PolicyRef,
    classification_contract_ref: ClassificationContractRef,
    authorizing_evidence_contract_ref: EvidenceContractRef,
    authorization_subject_profile_ref: SubjectProfileRef,
    input_schema_ref: SchemaRef,
    output_schema_ref: SchemaRef,
}

impl EffectiveRefs {
    /// The exact frozen reference set — every decision carries the
    /// same effective references.
    pub fn frozen() -> Self {
        Self {
            decision_contract_ref: DecisionContractRef::frozen(),
            authority_ref: AuthorityRefWire::frozen_authority(),
            policy_ref: PolicyRef::frozen(),
            classification_contract_ref: ClassificationContractRef::frozen_classification(),
            authorizing_evidence_contract_ref: EvidenceContractRef::frozen_evidence(),
            authorization_subject_profile_ref: SubjectProfileRef::frozen(),
            input_schema_ref: SchemaRef::frozen_input(),
            output_schema_ref: SchemaRef::frozen_output(),
        }
    }

    /// The decision contract reference.
    pub const fn decision_contract_ref(&self) -> &DecisionContractRef {
        &self.decision_contract_ref
    }

    /// The authority reference.
    pub const fn authority_ref(&self) -> &AuthorityRefWire {
        &self.authority_ref
    }

    /// The policy reference.
    pub const fn policy_ref(&self) -> &PolicyRef {
        &self.policy_ref
    }

    /// The classification contract reference.
    pub const fn classification_contract_ref(&self) -> &ClassificationContractRef {
        &self.classification_contract_ref
    }

    /// The authorizing-evidence contract reference.
    pub const fn authorizing_evidence_contract_ref(&self) -> &EvidenceContractRef {
        &self.authorizing_evidence_contract_ref
    }

    /// The authorization-subject-profile reference.
    pub const fn authorization_subject_profile_ref(&self) -> &SubjectProfileRef {
        &self.authorization_subject_profile_ref
    }

    /// The input schema reference.
    pub const fn input_schema_ref(&self) -> &SchemaRef {
        &self.input_schema_ref
    }

    /// The output schema reference.
    pub const fn output_schema_ref(&self) -> &SchemaRef {
        &self.output_schema_ref
    }
}

/// The closed derived-artifact requirements (the exact seven-member
/// transform checklist every `transform-required` outcome carries).
pub const TRANSFORM_REQUIREMENTS: [&str; 7] = [
    "new-artifact",
    "new-provenance",
    "retain-exact-source-authority-and-policy-refs",
    "retain-source-classification-refs",
    "record-applied-transforms",
    "record-declassification-or-aggregation-decision",
    "reevaluate-derived-output",
];

/// The exact decision contract version every declassification and
/// aggregation decision record must carry.
pub const DECISION_VERSION: &str = "0.2.16";

/// The closed export-decision output: the exact member set of the
/// frozen strict output schema.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDecisionOutput {
    decision: ExportDecision,
    reason_codes: Vec<String>,
    required_transforms: Vec<String>,
    effective_refs: EffectiveRefs,
    derived_artifact_requirements: Vec<String>,
    source_transfer_allowed: bool,
}

impl ExportDecisionOutput {
    /// Assemble the closed output. Every effective reference is the
    /// exact frozen value; nothing input-derived can enter here.
    pub fn new(
        decision: ExportDecision,
        reason_codes: Vec<String>,
        required_transforms: Vec<String>,
        derived_artifact_requirements: Vec<String>,
        source_transfer_allowed: bool,
    ) -> Self {
        Self {
            decision,
            reason_codes,
            required_transforms,
            effective_refs: EffectiveRefs::frozen(),
            derived_artifact_requirements,
            source_transfer_allowed,
        }
    }

    /// An exact one-reason deny; `transform-required` never routes
    /// here, and a deny never allows transfer.
    pub fn deny(reason_code: &str) -> Self {
        Self::new(
            ExportDecision::Deny,
            vec![reason_code.to_owned()],
            Vec::new(),
            Vec::new(),
            false,
        )
    }

    /// The exact `transform-required` outcome for one operation: the
    /// disposition reason, the required transforms, the full derived
    /// checklist, and never source transfer.
    pub fn transform_required(required_transforms: Vec<String>) -> Self {
        Self::new(
            ExportDecision::TransformRequired,
            vec!["disposition.transform-required".to_owned()],
            required_transforms,
            TRANSFORM_REQUIREMENTS
                .iter()
                .map(|entry| (*entry).to_owned())
                .collect(),
            false,
        )
    }

    /// The decision.
    pub const fn decision(&self) -> ExportDecision {
        self.decision
    }

    /// The stable reason codes (never empty).
    pub fn reason_codes(&self) -> &[String] {
        &self.reason_codes
    }

    /// The required transforms.
    pub fn required_transforms(&self) -> &[String] {
        &self.required_transforms
    }

    /// The effective exact contract references.
    pub const fn effective_refs(&self) -> &EffectiveRefs {
        &self.effective_refs
    }

    /// The derived-artifact requirements.
    pub fn derived_artifact_requirements(&self) -> &[String] {
        &self.derived_artifact_requirements
    }

    /// Whether the evaluated source may transfer.
    pub const fn source_transfer_allowed(&self) -> bool {
        self.source_transfer_allowed
    }
}

impl serde::Serialize for ExportDecision {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The allow output carries the exact closed wire shape with the
    /// frozen effective references.
    #[test]
    fn allow_output_has_the_exact_wire_shape() {
        let output = ExportDecisionOutput::new(
            ExportDecision::Allow,
            vec!["policy.allow".to_owned()],
            Vec::new(),
            Vec::new(),
            true,
        );
        let wire = serde_json::to_value(&output).unwrap();
        let object = wire.as_object().unwrap();
        assert_eq!(object.len(), 6);
        assert_eq!(object["decision"], json!("allow"));
        assert_eq!(object["reasonCodes"], json!(["policy.allow"]));
        assert_eq!(object["requiredTransforms"], json!([]));
        assert_eq!(object["derivedArtifactRequirements"], json!([]));
        assert_eq!(object["sourceTransferAllowed"], json!(true));
        let refs_json = object["effectiveRefs"].as_object().unwrap();
        assert_eq!(refs_json.len(), 8);
        assert_eq!(refs_json["policyRef"]["version"], json!("0.3.2"));
        assert_eq!(
            refs_json["classificationContractRef"]["version"],
            json!("0.2.16")
        );
        assert_eq!(
            refs_json["inputSchemaRef"]["schemaId"],
            json!("dev.lekalo.privacy-export-input-schema")
        );
        assert_eq!(refs_json["outputSchemaRef"]["version"], json!("0.3.2"));
    }

    /// The deny shortcut is the exact one-reason shape and never
    /// allows transfer.
    #[test]
    fn deny_is_exact_and_fails_closed() {
        let output = ExportDecisionOutput::deny("artifact-kind.unknown");
        assert_eq!(output.decision(), ExportDecision::Deny);
        assert_eq!(output.reason_codes(), ["artifact-kind.unknown"]);
        assert!(output.required_transforms().is_empty());
        assert!(!output.source_transfer_allowed());
        assert_eq!(
            serde_json::to_value(&output).unwrap()["decision"],
            json!("deny")
        );
    }

    /// The transform-required shortcut carries the full derived
    /// checklist and never authorizes source transfer.
    #[test]
    fn transform_required_carries_the_full_checklist() {
        let output = ExportDecisionOutput::transform_required(vec!["redact-content".to_owned()]);
        assert_eq!(output.decision(), ExportDecision::TransformRequired);
        assert_eq!(output.reason_codes(), ["disposition.transform-required"]);
        assert_eq!(output.required_transforms(), ["redact-content"]);
        assert_eq!(
            output.derived_artifact_requirements(),
            TRANSFORM_REQUIREMENTS
        );
        assert!(!output.source_transfer_allowed());
    }

    /// The decision vocabulary round-trips and refuses unknown
    /// spellings.
    #[test]
    fn decision_vocabulary_round_trips() {
        for decision in [
            ExportDecision::Allow,
            ExportDecision::Deny,
            ExportDecision::TransformRequired,
        ] {
            assert_eq!(ExportDecision::parse(decision.as_str()), Some(decision));
        }
        assert_eq!(ExportDecision::parse("Allow"), None);
        assert_eq!(ExportDecision::parse("block"), None);
        assert_eq!(DECISION_VERSION, "0.2.16");
    }
}
