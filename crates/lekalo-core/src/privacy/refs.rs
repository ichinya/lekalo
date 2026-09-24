//! The frozen #120 contract references and their pinned digests.
//!
//! Every constant here is frozen: the values are the exact accepted
//! custody anchors of `dev.lekalo.privacy-export-policy@0.3.2` and its
//! pinned satellite contracts. Nothing in this module is ever
//! regenerated or rewritten by the runtime; the bytes behind the
//! digests are embedded from `contracts/` and verified at load
//! ([`super::context::TrustedContext`]).

use super::types::{
    AuthorityRef, ClassificationContractRef, DecisionContractRef, EvidenceContractRef, PolicyRef,
    SchemaRef, SubjectProfileRef,
};

/// The exact SHA-256 of the embedded accepted-policy manifest bytes.
pub const TRUSTED_MANIFEST_SHA256: &str =
    "760f64bac4f2dd3e62e68b92a97f251316a215278a841d32b9e5b98a21302bf8";
/// The exact SHA-256 of the embedded accepted policy bytes.
pub const POLICY_RAW_SHA256: &str =
    "1fb9047934146e4ec76029b9c2c00b9fac4f605193911147974869b8794ab7dc";
/// The exact SHA-256 of the embedded classification decision contract bytes.
pub const CLASSIFICATION_CONTRACT_RAW_SHA256: &str =
    "78de535f02b6a8065579b43798ed650849aca0b7edf1aa08b2e0219fc744dde6";
/// The exact SHA-256 of the embedded authorizing-evidence contract bytes.
pub const AUTHORIZING_EVIDENCE_RAW_SHA256: &str =
    "cba51a4d9ae21a8d6ad7ebcb98f63410d918b0308ebcbdce1a165e084f52957e";
/// The exact SHA-256 of the embedded authorization-subject-profile bytes.
pub const AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256: &str =
    "11c3c6ddea482ba5cf1bca27f08d602801d7739f1bd57ed5efb15e100584dec0";
/// The exact SHA-256 of the embedded strict input schema bytes.
pub const INPUT_SCHEMA_RAW_SHA256: &str =
    "e0e95cc56db5758914aa34d12b3d2831954bc9df81d61184d37941d923c63581";
/// The exact SHA-256 of the embedded strict output schema bytes.
pub const OUTPUT_SCHEMA_RAW_SHA256: &str =
    "446c0a52433b6f4c265486a9d9736953436a883b120db56540b881dfd6f2b384";
/// The exact SHA-256 of the embedded CLI startup-error schema bytes.
pub const CLI_ERROR_SCHEMA_RAW_SHA256: &str =
    "46305d2c886f81e6793f4981814d1bb940bd4fffe0a02d25f6d0a615ea7fdf57";
/// The exact SHA-256 of the embedded classification-ref schema bytes.
pub const CLASSIFICATION_SCHEMA_RAW_SHA256: &str =
    "a6d871bb9159e104667afe7543423d665dd3f30498e22b0d7ae63426da8f2741";
/// The exact SHA-256 of the embedded authority matrix bytes.
pub const AUTHORITY_RAW_SHA256: &str =
    "7ae6454ea20f7b61202d368411ef9bff4e70af96f1f2a408c209d84fe9722f80";

/// The policy identity `dev.lekalo.privacy-export-policy@0.3.2`.
pub const POLICY_ID: &str = "dev.lekalo.privacy-export-policy";
/// The authority contract identity `dev.lekalo.authority-matrix`.
pub const AUTHORITY_CONTRACT_ID: &str = "dev.lekalo.authority-matrix";
/// The classification decision contract identity.
pub const CLASSIFICATION_CONTRACT_ID: &str = "dev.lekalo.privacy-classification-decision";
/// The authorizing-evidence contract identity.
pub const AUTHORIZING_EVIDENCE_CONTRACT_ID: &str = "dev.lekalo.privacy-authorizing-evidence";
/// The authorization-subject-profile identity.
pub const AUTHORIZATION_SUBJECT_PROFILE_ID: &str =
    "dev.lekalo.privacy-authorization-subject-profile";
/// The decision-contract identity.
pub const DECISION_CONTRACT_ID: &str = "dev.lekalo.privacy-export-decision";
/// The strict input schema identity.
pub const INPUT_SCHEMA_ID: &str = "dev.lekalo.privacy-export-input-schema";
/// The strict output schema identity.
pub const OUTPUT_SCHEMA_ID: &str = "dev.lekalo.privacy-export-output-schema";
/// The CLI startup-error schema identity.
pub const CLI_ERROR_SCHEMA_ID: &str = "dev.lekalo.privacy-cli-error-schema";
/// The classification-ref schema identity.
pub const CLASSIFICATION_SCHEMA_ID: &str = "dev.lekalo.privacy-classification-decision-schema";

/// The pinned policy semantic digest (canonical projection identity).
pub const POLICY_DIGEST: &str =
    "sha256:5a80966fa628fd4c9452325d34e7191f40ebb7a9cb49185c91d413861fb18384";
/// The policy family version.
pub const POLICY_VERSION: &str = "0.3.2";
/// The authority matrix version.
pub const AUTHORITY_VERSION: &str = "0.3.2";
/// The decision-contract family version shared by decision, evidence,
/// subject-profile, classification, and schema satellites.
pub const DECISION_FAMILY_VERSION: &str = "0.2.16";

/// The complete frozen reference set of the accepted #120 family,
/// with every member carried as exact owned bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrozenRefs {
    /// The accepted policy.
    pub policy: PolicyRef,
    /// The exact authority matrix.
    pub authority: AuthorityRef,
    /// The classification decision contract.
    pub classification_contract: ClassificationContractRef,
    /// The authorizing-evidence registry contract.
    pub authorizing_evidence_contract: EvidenceContractRef,
    /// The canonical authorization-subject profile.
    pub authorization_subject_profile: SubjectProfileRef,
    /// The evaluated decision contract.
    pub decision: DecisionContractRef,
    /// The strict input schema.
    pub input_schema: SchemaRef,
    /// The strict output schema.
    pub output_schema: SchemaRef,
    /// The separate CLI startup-error schema.
    pub cli_error_schema: SchemaRef,
    /// The classification-ref schema.
    pub classification_schema: SchemaRef,
}

impl FrozenRefs {
    /// The exact frozen reference set. Every value is a compile-time
    /// constant; nothing here is ever derived from input.
    pub fn frozen() -> Self {
        Self {
            policy: PolicyRef::frozen(),
            authority: AuthorityRef::frozen_authority(),
            classification_contract: ClassificationContractRef::frozen_classification(),
            authorizing_evidence_contract: EvidenceContractRef::frozen_evidence(),
            authorization_subject_profile: SubjectProfileRef::frozen(),
            decision: DecisionContractRef::frozen(),
            input_schema: SchemaRef::frozen_input(),
            output_schema: SchemaRef::frozen_output(),
            cli_error_schema: SchemaRef::frozen_cli_error(),
            classification_schema: SchemaRef::frozen_classification_schema(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digest::sha256_hex;

    /// The frozen references are the exact accepted values.
    #[test]
    fn frozen_refs_carry_the_accepted_identities() {
        let refs = FrozenRefs::frozen();
        assert_eq!(refs.policy.policy_id(), "dev.lekalo.privacy-export-policy");
        assert_eq!(refs.policy.version(), "0.3.2");
        assert_eq!(
            refs.policy.digest(),
            "sha256:5a80966fa628fd4c9452325d34e7191f40ebb7a9cb49185c91d413861fb18384"
        );
        assert_eq!(refs.authority.contract_id(), "dev.lekalo.authority-matrix");
        assert_eq!(
            refs.authority.digest(),
            "sha256:7ae6454ea20f7b61202d368411ef9bff4e70af96f1f2a408c209d84fe9722f80"
        );
        assert_eq!(
            refs.decision.contract_id(),
            "dev.lekalo.privacy-export-decision"
        );
        assert_eq!(refs.decision.version(), "0.2.16");
        assert_eq!(refs.input_schema.version(), "0.3.2");
        assert_eq!(refs.cli_error_schema.version(), "0.2.16");
        assert_eq!(
            refs.authorization_subject_profile.profile_id(),
            "dev.lekalo.privacy-authorization-subject-profile"
        );
    }

    /// Every pinned raw digest is exact lowercase-hex SHA-256 spelling.
    #[test]
    fn pinned_raw_digests_are_hex_sha256() {
        for digest in [
            TRUSTED_MANIFEST_SHA256,
            POLICY_RAW_SHA256,
            CLASSIFICATION_CONTRACT_RAW_SHA256,
            AUTHORIZING_EVIDENCE_RAW_SHA256,
            AUTHORIZATION_SUBJECT_PROFILE_RAW_SHA256,
            INPUT_SCHEMA_RAW_SHA256,
            OUTPUT_SCHEMA_RAW_SHA256,
            CLI_ERROR_SCHEMA_RAW_SHA256,
            CLASSIFICATION_SCHEMA_RAW_SHA256,
            AUTHORITY_RAW_SHA256,
        ] {
            assert_eq!(digest.len(), 64);
            assert!(digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
        }
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
